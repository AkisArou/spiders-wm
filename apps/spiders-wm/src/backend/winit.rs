use smithay::backend::egl::EGLDevice;
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::{ImportDma, ImportMemWl};
use smithay::backend::winit::{self, WinitEvent};
use smithay::output::{Mode, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::reexports::winit::dpi::LogicalSize;
use smithay::reexports::winit::window::Window;
use smithay::utils::{Physical, Size, Transform};
use smithay::wayland::dmabuf::DmabufFeedbackBuilder;
use tracing::warn;

use crate::backend::BackendState;
use crate::backend::output::{OutputRegistration, register_output, sync_output_to_runtime};
use crate::state::SpidersWm;

pub(crate) fn init_winit(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    state: &mut SpidersWm,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init_from_attributes::<GlesRenderer>(
        Window::default_attributes()
            .with_inner_size(LogicalSize::new(1280.0, 800.0))
            .with_title("spiders-wm-winit")
            .with_visible(true),
    )?;

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
        Some(state.dmabuf_state.create_global_with_default_feedback::<SpidersWm>(
            &state.display_handle,
            default_feedback,
        ))
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

    let reported_size = backend.window_size();
    state.backend = Some(BackendState::Winit(backend));

    let mode = Mode {
        size: sanitize_winit_output_size(reported_size).unwrap_or_else(|| {
            warn!(
                reported_width = reported_size.w,
                reported_height = reported_size.h,
                fallback_width = DEFAULT_WINIT_OUTPUT_WIDTH,
                fallback_height = DEFAULT_WINIT_OUTPUT_HEIGHT,
                "wm ignoring tiny startup winit output size"
            );
            default_winit_output_size()
        }),
        refresh: 60_000,
    };

    let output = register_output(
        state,
        OutputRegistration {
            output_id: "winit".into(),
            output_name: "winit".into(),
            mode,
            transform: Transform::Flipped180,
            location: (0, 0).into(),
            physical: PhysicalProperties {
                size: (0, 0).into(),
                subpixel: Subpixel::Unknown,
                make: "Smithay".into(),
                model: "Winit".into(),
                serial_number: "Unknown".into(),
            },
        },
    );

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop.handle().insert_source(winit, move |event, _, state| match event {
        WinitEvent::Resized { size, .. } => {
            let Some(size) = sanitize_winit_output_size(size) else {
                warn!(
                    reported_width = size.w,
                    reported_height = size.h,
                    "wm ignoring tiny winit resize event"
                );
                return;
            };
            let mode = Mode { size, refresh: 60_000 };
            output.change_current_state(Some(mode), None, None, None);
            state.space.refresh();
            state.refresh_fractional_scale_for_mapped_surfaces();
            sync_output_to_runtime(state, "winit".into(), "winit".into(), mode);
            state.schedule_relayout();
        }
        WinitEvent::Input(event) => state.process_input_event(event),
        WinitEvent::Redraw => state.render_output_frame(&output, &mut damage_tracker),
        WinitEvent::CloseRequested => state.loop_signal.stop(),
        _ => {}
    })?;

    Ok(())
}

const DEFAULT_WINIT_OUTPUT_WIDTH: i32 = 1280;
const DEFAULT_WINIT_OUTPUT_HEIGHT: i32 = 800;
const MIN_VALID_WINIT_OUTPUT_EDGE: i32 = 64;

pub(crate) fn default_winit_output_size() -> Size<i32, Physical> {
    Size::from((DEFAULT_WINIT_OUTPUT_WIDTH, DEFAULT_WINIT_OUTPUT_HEIGHT))
}

pub(crate) fn sanitize_winit_output_size(size: Size<i32, Physical>) -> Option<Size<i32, Physical>> {
    (size.w >= MIN_VALID_WINIT_OUTPUT_EDGE && size.h >= MIN_VALID_WINIT_OUTPUT_EDGE).then_some(size)
}
