#[cfg(feature = "tty-preview")]
use smithay::backend::drm::DrmNode;
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::element::Kind;
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::gles::GlesRenderer;
#[cfg(feature = "tty-preview")]
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
#[cfg(feature = "tty-preview")]
use smithay::wayland::compositor::with_states;

#[cfg(feature = "tty-preview")]
use crate::backend::BackendState;
#[cfg(feature = "tty-preview")]
use crate::frame_sync::SnapshotRenderElement;
use crate::state::SpidersWm;

#[cfg(feature = "tty-preview")]
smithay::backend::renderer::element::render_elements! {
    pub(crate) TtySceneRenderElement<=GlesRenderer>;
    Space=smithay::desktop::space::SpaceRenderElements<GlesRenderer, smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement<GlesRenderer>>,
    Snapshot=SnapshotRenderElement,
    CursorSurface=WaylandSurfaceRenderElement<GlesRenderer>,
    CursorFallback=SolidColorRenderElement,
}

#[cfg(feature = "tty-preview")]
impl SpidersWm {
    pub(crate) fn try_tty_blank_frame(&mut self) {
        self.notify_blocker_cleared();
        self.prune_completed_closing_overlays();

        let targets = match self.backend.as_ref() {
            Some(BackendState::Tty(backend)) => {
                let active = matches!(&backend.session, crate::backend::session::BackendSession::Tty(session) if session.active);
                active.then(|| {
                    backend
                        .outputs
                        .iter()
                        .filter_map(|output| {
                            output
                                .drm_node
                                .map(|node| (output.output.clone(), node, output.connector))
                        })
                        .collect::<Vec<_>>()
                })
            }
            _ => None,
        };
        let Some(targets) = targets else {
            return;
        };

        let mut presented_outputs = Vec::new();

        for (output, node, connector) in targets {
            let elements = self.tty_collect_scene_elements(&output, node);

            let Some(BackendState::Tty(backend)) = self.backend.as_mut() else {
                return;
            };
            let Some(device) = backend.drm.devices.iter_mut().find(|device| device.node == node)
            else {
                continue;
            };
            let Some(render_node) = device.render.render_node else {
                continue;
            };
            let Some(gpu_manager) = device.render.gpu_manager.as_mut() else {
                continue;
            };

            let Ok(mut renderer) = gpu_manager.single_renderer(&render_node) else {
                continue;
            };
            let Some(surface) =
                device.surfaces.iter_mut().find(|surface| surface.connector == connector)
            else {
                continue;
            };
            let Some(compositor) = surface.compositor.as_mut() else {
                continue;
            };

            match compositor.render_frame(
                renderer.as_mut(),
                &elements,
                [0.08, 0.08, 0.1, 1.0],
                smithay::backend::drm::compositor::FrameFlags::DEFAULT,
            ) {
                Ok(frame) => {
                    if frame.is_empty {
                        continue;
                    }
                    if compositor.queue_frame(()).is_ok() {
                        presented_outputs.push(output.clone());
                    }
                }
                Err(_) => {}
            }
        }

        if !presented_outputs.is_empty() {
            self.frame_sync.mark_closing_overlays_presented();
            for output in &presented_outputs {
                self.send_frames_for_windows(output);
            }
            self.space.refresh();
            self.popups.cleanup();
            let _ = self.display_handle.flush_clients();
        }
    }

    fn tty_collect_scene_elements(
        &mut self,
        output: &smithay::output::Output,
        node: DrmNode,
    ) -> Vec<TtySceneRenderElement> {
        let mut backend = match self.backend.take() {
            Some(BackendState::Tty(backend)) => backend,
            Some(other) => {
                self.backend = Some(other);
                return Vec::new();
            }
            None => return Vec::new(),
        };

        let Some(primary_device) =
            backend.drm.devices.iter_mut().find(|device| device.node == node)
        else {
            self.backend = Some(BackendState::Tty(backend));
            return Vec::new();
        };
        let Some(render_node) = primary_device.render.render_node else {
            self.backend = Some(BackendState::Tty(backend));
            return Vec::new();
        };
        let Some(gpu_manager) = primary_device.render.gpu_manager.as_mut() else {
            self.backend = Some(BackendState::Tty(backend));
            return Vec::new();
        };
        let Ok(mut renderer) = gpu_manager.single_renderer(&render_node) else {
            self.backend = Some(BackendState::Tty(backend));
            return Vec::new();
        };

        let scale = smithay::utils::Scale::from(output.current_scale().fractional_scale());
        let mut elements = smithay::desktop::space::space_render_elements::<
            _,
            smithay::desktop::Window,
            _,
        >(renderer.as_mut(), [&self.space], output, 1.0)
        .unwrap_or_default()
        .into_iter()
        .map(TtySceneRenderElement::Space)
        .collect::<Vec<_>>();

        let overlay_elements = self.frame_sync.render_elements(renderer.as_mut(), scale, 1.0);
        elements.extend(overlay_elements.into_iter().map(TtySceneRenderElement::Snapshot));
        elements.extend(self.tty_cursor_elements(renderer.as_mut(), output));

        self.backend = Some(BackendState::Tty(backend));
        elements
    }

    fn tty_cursor_elements(
        &self,
        renderer: &mut GlesRenderer,
        output: &smithay::output::Output,
    ) -> Vec<TtySceneRenderElement> {
        if matches!(self.cursor_image_status, CursorImageStatus::Hidden) {
            return Vec::new();
        }
        let Some(output_geometry) = self.output_geometry_for(output) else {
            return Vec::new();
        };
        let local = self.pointer_location - output_geometry.loc.to_f64();
        if local.x < 0.0
            || local.y < 0.0
            || local.x >= output_geometry.size.w as f64
            || local.y >= output_geometry.size.h as f64
        {
            return Vec::new();
        }

        let scale = smithay::utils::Scale::from(output.current_scale().fractional_scale());

        if let CursorImageStatus::Surface(surface) = &self.cursor_image_status {
            let hotspot = with_states(surface, |states| {
                states
                    .data_map
                    .get::<CursorImageSurfaceData>()
                    .and_then(|data| data.lock().ok())
                    .map(|attributes| attributes.hotspot)
                    .unwrap_or_default()
            });
            let origin =
                (local - hotspot.to_f64()).to_i32_round::<i32>().to_physical_precise_round(scale);
            let elements = render_elements_from_surface_tree(
                renderer,
                surface,
                origin,
                scale,
                1.0,
                Kind::Cursor,
            )
            .into_iter()
            .map(TtySceneRenderElement::CursorSurface)
            .collect::<Vec<_>>();
            if !elements.is_empty() {
                return elements;
            }
        }

        let buffer = SolidColorBuffer::new((10, 16), [0.95, 0.95, 0.98, 0.9]);
        vec![TtySceneRenderElement::CursorFallback(SolidColorRenderElement::from_buffer(
            &buffer,
            (local.x as i32, local.y as i32),
            1.0,
            1.0,
            Kind::Cursor,
        ))]
    }
}

#[cfg(not(feature = "tty-preview"))]
impl SpidersWm {
    #[allow(dead_code)]
    pub(crate) fn try_tty_blank_frame(&mut self) {}
}
