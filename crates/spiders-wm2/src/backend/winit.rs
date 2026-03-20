use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement,
            gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Rectangle, Transform},
};

use crate::{actions, model::OutputId, runtime::SpidersWm2};

pub fn init_winit(
    event_loop: &mut EventLoop<SpidersWm2>,
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

    let _global = output.create_global::<SpidersWm2>(&state.runtime.display_handle);

    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );

    output.set_preferred(mode);

    state.runtime.smithay.space.map_output(&output, (0, 0));

    actions::register_output(
        &mut state.app.topology,
        &mut state.app.wm,
        OutputId::from("1"),
        "winit".to_string(),
        (mode.size.w as u32, mode.size.h as u32),
    );
    actions::sync_active_workspace_to_output(&mut state.app.topology, &mut state.app.wm);
    state.refresh_active_workspace();

    let mut damager_tracker = OutputDamageTracker::from_output(&output);

    backend.window().request_redraw();

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
                if actions::update_output_logical_size(
                    &mut state.app.topology,
                    &OutputId::from("1"),
                    (size.w as u32, size.h as u32),
                ) {
                    state.refresh_active_workspace();
                } else {
                    state
                        .runtime
                        .render_plan
                        .mark_output_dirty(OutputId::from("1"));
                }
            }
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => {
                state.cleanup_dead_windows();
                state.maybe_commit_pending_transaction();

                if state.runtime.render_plan.can_skip_redraw_until_commit() {
                    state.runtime.smithay.space.refresh();
                    state.runtime.smithay.popups.cleanup();
                    let _ = state.runtime.display_handle.flush_clients();
                    backend.window().request_redraw();
                    return;
                }

                let outputs = state
                    .runtime
                    .smithay
                    .space
                    .outputs()
                    .cloned()
                    .collect::<Vec<_>>();
                let has_dirty_output = outputs.iter().any(|output| {
                    output_id_for_space_output(state, output)
                        .map(|output_id| state.runtime.render_plan.should_render_output(&output_id))
                        .unwrap_or(false)
                });

                if !has_dirty_output {
                    state.runtime.smithay.space.refresh();
                    state.runtime.smithay.popups.cleanup();
                    let _ = state.runtime.display_handle.flush_clients();
                    backend.window().request_redraw();
                    return;
                }

                for render_output in outputs {
                    let Some(output_id) = output_id_for_space_output(state, &render_output) else {
                        continue;
                    };

                    if !state.runtime.render_plan.should_render_output(&output_id) {
                        continue;
                    }

                    render_output_frame(&mut backend, state, &render_output, &mut damager_tracker);
                    state.runtime.render_plan.clear_output(&output_id);
                }

                state.runtime.smithay.space.elements().for_each(|window| {
                    window.send_frame(
                        &output,
                        state.runtime.start_time.elapsed(),
                        Some(Duration::ZERO),
                        |_, _| Some(output.clone()),
                    );
                });

                state.runtime.smithay.space.refresh();
                state.runtime.smithay.popups.cleanup();
                let _ = state.runtime.display_handle.flush_clients();
                backend.window().request_redraw();
            }
            WinitEvent::CloseRequested => {
                state.runtime.loop_signal.stop();
            }
            _ => {}
        })?;

    Ok(())
}

fn output_id_for_space_output(state: &SpidersWm2, output: &Output) -> Option<OutputId> {
    let output_name = output.name();
    state
        .app
        .topology
        .outputs
        .iter()
        .find_map(|(output_id, node)| (node.name == output_name).then(|| output_id.clone()))
}

fn render_output_frame(
    backend: &mut winit::WinitGraphicsBackend<GlesRenderer>,
    state: &mut SpidersWm2,
    output: &Output,
    damage_tracker: &mut OutputDamageTracker,
) {
    let size = backend.window_size();
    let damage = Rectangle::from_size(size);

    {
        let (renderer, mut framebuffer) = backend.bind().unwrap();

        smithay::desktop::space::render_output::<_, WaylandSurfaceRenderElement<GlesRenderer>, _, _>(
            output,
            renderer,
            &mut framebuffer,
            1.0,
            0,
            [&state.runtime.smithay.space],
            &[],
            damage_tracker,
            [0.08, 0.09, 0.11, 1.0],
        )
        .unwrap();
    }

    backend.submit(Some(&[damage])).unwrap();
}
