#[cfg(feature = "tty-preview")]
use drm_fourcc::DrmFourcc;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::DrmNode;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::compositor::DrmCompositor;
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::ImportDma;
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::multigpu::gbm::GbmGlesBackend;
#[cfg(feature = "tty-preview")]
use smithay::output::OutputModeSource;
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::Device as ControlDevice;
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::crtc;
#[cfg(feature = "tty-preview")]
use tracing::info;

#[cfg(feature = "tty-preview")]
use crate::backend::BackendState;
#[cfg(feature = "tty-preview")]
use crate::backend::TtyBackendState;
#[cfg(feature = "tty-preview")]
use crate::state::SpidersWm;

#[cfg(feature = "tty-preview")]
type TtyGpuManager = smithay::backend::renderer::multigpu::GpuManager<
    GbmGlesBackend<
        smithay::backend::renderer::gles::GlesRenderer,
        smithay::backend::drm::DrmDeviceFd,
    >,
>;

#[cfg(feature = "tty-preview")]
pub(crate) fn initialize_drm_surfaces(
    seat_name: &str,
    backend: &mut TtyBackendState,
) -> Result<(), Box<dyn std::error::Error>> {
    for device in &mut backend.drm.devices {
        for output in &backend.outputs {
            let Some(output_node) = output.drm_node else {
                continue;
            };
            if output_node != device.node {
                continue;
            }
            if device.surfaces.iter().any(|surface| surface.connector == output.connector) {
                continue;
            }

            let Some(crtc) = select_crtc_for_connector(&device.device, output.connector)? else {
                info!(seat = %seat_name, node = ?device.node, connector = ?output.connector, connector_name = output.connector_name, "tty preview found no compatible crtc for connector");
                continue;
            };

            match device.device.create_surface(crtc, output.drm_mode, &[output.connector]) {
                Ok(surface) => {
                    device.surfaces.push(crate::backend::drm::DrmSurfaceRecord {
                        connector: output.connector,
                        crtc,
                        surface: Some(surface),
                        compositor: None,
                    });
                }
                Err(error) => {
                    info!(seat = %seat_name, node = ?device.node, connector = ?output.connector, connector_name = output.connector_name, ?crtc, %error, "tty preview failed to create drm surface candidate");
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "tty-preview")]
pub(crate) fn initialize_tty_device_surfaces(
    seat_name: &str,
    backend: &mut TtyBackendState,
    node: DrmNode,
) -> Result<(), Box<dyn std::error::Error>> {
    for device in &mut backend.drm.devices {
        if device.node != node {
            continue;
        }
        for output in &backend.outputs {
            if output.drm_node != Some(node) {
                continue;
            }
            if device.surfaces.iter().any(|surface| surface.connector == output.connector) {
                continue;
            }

            let Some(crtc) = select_crtc_for_connector(&device.device, output.connector)? else {
                continue;
            };

            match device.device.create_surface(crtc, output.drm_mode, &[output.connector]) {
                Ok(surface) => {
                    device.surfaces.push(crate::backend::drm::DrmSurfaceRecord {
                        connector: output.connector,
                        crtc,
                        surface: Some(surface),
                        compositor: None,
                    });
                }
                Err(error) => {
                    info!(seat = %seat_name, node = ?device.node, connector = ?output.connector, connector_name = output.connector_name, ?crtc, %error, "tty preview failed to recreate drm surface candidate");
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "tty-preview")]
pub(crate) fn reset_tty_device_scanout_state(backend: &mut TtyBackendState, node: DrmNode) {
    for device in &mut backend.drm.devices {
        if device.node != node {
            continue;
        }
        for surface in &mut device.surfaces {
            surface.compositor = None;
            let _ = surface.surface.take();
        }
        device.surfaces.clear();
    }
}

#[cfg(feature = "tty-preview")]
pub(crate) fn initialize_tty_renderer_state(
    state: &mut SpidersWm,
) -> Result<(), Box<dyn std::error::Error>> {
    let nodes = {
        let Some(BackendState::Tty(backend)) = state.backend.as_ref() else {
            return Ok(());
        };
        backend.drm.devices.iter().map(|device| device.node).collect::<Vec<_>>()
    };

    for node in nodes {
        initialize_tty_renderer_state_for_node(state, node)?;
    }

    Ok(())
}

#[cfg(feature = "tty-preview")]
pub(crate) fn initialize_tty_renderer_state_for_node(
    state: &mut SpidersWm,
    node: DrmNode,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(BackendState::Tty(backend)) = state.backend.as_mut() else {
        return Ok(());
    };
    let Some(primary_device) = backend.drm.devices.iter_mut().find(|device| device.node == node)
    else {
        return Ok(());
    };
    let Some(render_node) = primary_device.render.render_node else {
        return Ok(());
    };

    if primary_device.render.gpu_manager.is_none() {
        let mut api = GbmGlesBackend::default();
        api.add_node(render_node, primary_device.render.gbm.clone())?;
        primary_device.render.gpu_manager = Some(TtyGpuManager::new(api)?);
    }

    let gpu_manager = primary_device
        .render
        .gpu_manager
        .as_mut()
        .ok_or_else(|| std::io::Error::other("tty gpu manager missing after initialization"))?;
    let mut renderer = gpu_manager.single_renderer(&render_node)?;

    if state.dmabuf_global.is_none() {
        let primary_formats = renderer.dmabuf_formats();
        let default_feedback = smithay::wayland::dmabuf::DmabufFeedbackBuilder::new(
            render_node.dev_id(),
            primary_formats.clone(),
        )
        .build()?;
        state.dmabuf_global =
            Some(state.dmabuf_state.create_global_with_default_feedback::<SpidersWm>(
                &state.display_handle,
                &default_feedback,
            ));
    }

    let renderer_formats =
        renderer.as_mut().egl_context().dmabuf_render_formats().iter().copied().collect::<Vec<_>>();

    for surface in &mut primary_device.surfaces {
        if surface.compositor.is_some() {
            continue;
        }

        let Some(surface_handle) = surface.surface.take() else {
            continue;
        };

        match DrmCompositor::new(
            OutputModeSource::Auto(
                backend
                    .outputs
                    .iter()
                    .find(|output| output.connector == surface.connector)
                    .map(|output| output.output.clone())
                    .ok_or_else(|| std::io::Error::other("missing tty output for drm surface"))?,
            ),
            surface_handle,
            None,
            primary_device.render.allocator.clone(),
            primary_device.render.framebuffer_exporter.clone(),
            [DrmFourcc::Argb8888, DrmFourcc::Xrgb8888],
            renderer_formats.clone(),
            primary_device.device.cursor_size(),
            Some(primary_device.render.gbm.clone()),
        ) {
            Ok(compositor) => {
                surface.compositor = Some(compositor);
            }
            Err(error) => {
                info!(node = ?primary_device.node, connector = ?surface.connector, crtc = ?surface.crtc, ?error, "tty preview failed to create drm compositor candidate");
            }
        }
    }

    Ok(())
}

#[cfg(feature = "tty-preview")]
fn select_crtc_for_connector(
    device: &smithay::backend::drm::DrmDevice,
    connector: smithay::reexports::drm::control::connector::Handle,
) -> Result<Option<crtc::Handle>, Box<dyn std::error::Error>> {
    let resources = device
        .resource_handles()
        .map_err(|error| std::io::Error::other(format!("failed to read drm resources: {error}")))?;
    let connector_info = device.get_connector(connector, true).map_err(|error| {
        std::io::Error::other(format!("failed to read connector info: {error}"))
    })?;

    if let Some(encoder) =
        connector_info.current_encoder().or_else(|| connector_info.encoders().first().copied())
    {
        let encoder_info = device.get_encoder(encoder).map_err(|error| {
            std::io::Error::other(format!("failed to read encoder info: {error}"))
        })?;
        if let Some(crtc) = encoder_info.crtc() {
            return Ok(Some(crtc));
        }

        let possible_crtcs = resources.filter_crtcs(encoder_info.possible_crtcs());
        if let Some(crtc) = possible_crtcs.first().copied() {
            return Ok(Some(crtc));
        }
    }

    Ok(resources.crtcs().first().copied())
}
