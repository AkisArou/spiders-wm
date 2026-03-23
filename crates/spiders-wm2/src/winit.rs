use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::backend::winit::{self, WinitEvent};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::utils::{Rectangle, Transform};

use crate::closing::Wm2RenderElements;
use crate::state::SpidersWm2;

pub fn init_winit(
    event_loop: &mut EventLoop<'static, SpidersWm2>,
    state: &mut SpidersWm2,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut backend, winit) = winit::init()?;

    let mode = Mode {
        size: backend.window_size(),
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
    let _global = output.create_global::<SpidersWm2>(&state.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));

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
                state.schedule_relayout();
            }
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => {
                state.notify_blocker_cleared();
                state.advance_closing_windows();
                state.advance_resize_overlays();

                let size = backend.window_size();
                let damage = Rectangle::from_size(size);

                {
                    let (renderer, mut framebuffer) =
                        backend.bind().expect("failed to bind winit backend");
                    state.refresh_window_snapshots(renderer);
                    let transition_elements = state.transition_render_elements();
                    smithay::desktop::space::render_output::<
                        _,
                        Wm2RenderElements,
                        _,
                        _,
                    >(
                        &output,
                        renderer,
                        &mut framebuffer,
                        1.0,
                        0,
                        [&state.space],
                        &transition_elements,
                        &mut damage_tracker,
                        [0.08, 0.08, 0.1, 1.0],
                    )
                    .expect("failed to render output");
                }

                backend
                    .submit(Some(&[damage]))
                    .expect("failed to submit frame");

                state.send_frames_for_windows(&output);

                state.space.refresh();
                state.popups.cleanup();
                let _ = state.display_handle.flush_clients();
                backend.window().request_redraw();
            }
            WinitEvent::CloseRequested => state.loop_signal.stop(),
            _ => {}
        })?;

    Ok(())
}
