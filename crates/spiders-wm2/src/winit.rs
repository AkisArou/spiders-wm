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

use crate::runtime::SpidersWm2;

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
            }
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => {
                let size = backend.window_size();
                let damage = Rectangle::from_size(size);

                {
                    let (renderer, mut framebuffer) = backend.bind().unwrap();

                    smithay::desktop::space::render_output::<
                        _,
                        WaylandSurfaceRenderElement<GlesRenderer>,
                        _,
                        _,
                    >(
                        &output,
                        renderer,
                        &mut framebuffer,
                        1.0,
                        0,
                        [&state.runtime.smithay.space],
                        &[],
                        &mut damager_tracker,
                        [0.08, 0.09, 0.11, 1.0],
                    )
                    .unwrap();
                }

                backend.submit(Some(&[damage])).unwrap();

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
        });

    Ok(())
}
// use std::time::Duration;
//
// use smithay::{
//     backend::{
//         renderer::{
//             damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement,
//             gles::GlesRenderer,
//         },
//         winit::{self, WinitEvent},
//     },
//     output::{Mode, Output, PhysicalProperties, Subpixel},
//     reexports::calloop::EventLoop,
//     utils::{Rectangle, Transform},
// };
//
// use crate::state::SpidersWm2;
//
// pub fn init_winit(
//     event_loop: &mut EventLoop<SpidersWm2>,
//     state: &mut SpidersWm2,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     let (mut backend, winit) = winit::init()?;
//
//     let mode = Mode {
//         size: backend.window_size(),
//         refresh: 60_000,
//     };
//
//     let output = Output::new(
//         "winit".to_string(),
//         PhysicalProperties {
//             size: (0, 0).into(),
//             subpixel: Subpixel::Unknown,
//             make: "Smithay".into(),
//             model: "Winit".into(),
//             serial_number: "Unknown".into(),
//         },
//     );
//     let _global = output.create_global::<SpidersWm2>(&state.display_handle);
//     output.change_current_state(
//         Some(mode),
//         Some(Transform::Flipped180),
//         None,
//         Some((0, 0).into()),
//     );
//     output.set_preferred(mode);
//
//     state.space.map_output(&output, (0, 0));
//
//     let mut damage_tracker = OutputDamageTracker::from_output(&output);
//     backend.window().request_redraw();
//
//     event_loop
//         .handle()
//         .insert_source(winit, move |event, _, state| match event {
//             WinitEvent::Resized { size, .. } => {
//                 output.change_current_state(
//                     Some(Mode {
//                         size,
//                         refresh: 60_000,
//                     }),
//                     None,
//                     None,
//                     None,
//                 );
//             }
//             WinitEvent::Input(event) => state.process_input_event(event),
//             WinitEvent::Redraw => {
//                 let size = backend.window_size();
//                 let damage = Rectangle::from_size(size);
//
//                 {
//                     let (renderer, mut framebuffer) = backend.bind().unwrap();
//                     smithay::desktop::space::render_output::<
//                         _,
//                         WaylandSurfaceRenderElement<GlesRenderer>,
//                         _,
//                         _,
//                     >(
//                         &output,
//                         renderer,
//                         &mut framebuffer,
//                         1.0,
//                         0,
//                         [&state.space],
//                         &[],
//                         &mut damage_tracker,
//                         [0.08, 0.09, 0.11, 1.0],
//                     )
//                     .unwrap();
//                 }
//
//                 backend.submit(Some(&[damage])).unwrap();
//
//                 state.space.elements().for_each(|window| {
//                     window.send_frame(
//                         &output,
//                         state.start_time.elapsed(),
//                         Some(Duration::ZERO),
//                         |_, _| Some(output.clone()),
//                     )
//                 });
//
//                 state.space.refresh();
//                 state.popups.cleanup();
//                 let _ = state.display_handle.flush_clients();
//                 backend.window().request_redraw();
//             }
//             WinitEvent::CloseRequested => {
//                 state.loop_signal.stop();
//             }
//             _ => {}
//         })?;
//
//     Ok(())
// }
