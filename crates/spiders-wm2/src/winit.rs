use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::{ImportDma, ImportMemWl};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::winit::{self, WinitEvent};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::utils::Transform;
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use tracing::warn;

use crate::runtime::RuntimeCommand;
use crate::state::SpidersWm;

pub fn init_winit(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    state: &mut SpidersWm,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init::<GlesRenderer>()?;

    state.shm_state.update_formats(backend.renderer().shm_formats());

    let render_node = EGLDevice::device_for_display(backend.renderer().egl_context().display())
        .and_then(|device| device.try_get_render_node());

    let dmabuf_default_feedback = match render_node {
        Ok(Some(node)) => {
            let dmabuf_formats = backend.renderer().dmabuf_formats();
            DmabufFeedbackBuilder::new(node.dev_id(), dmabuf_formats)
                .build()
                .map(Some)
                .map_err(|err| err.to_string())
        }
        Ok(None) => {
            warn!("failed to query render node, dmabuf will use v3");
            Ok(None)
        }
        Err(err) => {
            warn!(?err, "failed to query EGL render node, dmabuf will use v3");
            Ok(None)
        }
    }
    .expect("failed to build dmabuf feedback");

    state.dmabuf_global = if let Some(default_feedback) = dmabuf_default_feedback.as_ref() {
        Some(
            state
                .dmabuf_state
                .create_global_with_default_feedback::<SpidersWm>(
                    &state.display_handle,
                    default_feedback,
                ),
        )
    } else {
        let dmabuf_formats = backend.renderer().dmabuf_formats();
        if dmabuf_formats.iter().next().is_some() {
            Some(
                state
                    .dmabuf_state
                    .create_global::<SpidersWm>(&state.display_handle, dmabuf_formats),
            )
        } else {
            None
        }
    };

    state.backend = Some(backend);

    let mode = Mode {
        size: state
            .backend
            .as_ref()
            .expect("winit backend missing during init")
            .window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
            serial_number: "Unknown".into(),
        },
    );
    let _global = output.create_global::<SpidersWm>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));
    let _ = state.runtime().execute(RuntimeCommand::SyncOutput {
        output_id: "winit".into(),
        name: "winit".to_string(),
        logical_width: mode.size.w as u32,
        logical_height: mode.size.h as u32,
    });

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop
        .handle()
        .insert_source(winit, move |event, _, state| match event {
            WinitEvent::Resized { size, .. } => {
                output.change_current_state(
                    Some(Mode {
                        size,
                        refresh: 60_000,
                    }),
                    None,
                    None,
                    None,
                );
                let _ = state.runtime().execute(RuntimeCommand::SyncOutput {
                    output_id: "winit".into(),
                    name: "winit".to_string(),
                    logical_width: size.w as u32,
                    logical_height: size.h as u32,
                });
                state.schedule_relayout();
            }
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => state.render_output_frame(&output, &mut damage_tracker),
            WinitEvent::CloseRequested => state.loop_signal.stop(),
            _ => {}
        })?;

    Ok(())
}
