#[cfg(feature = "smithay-winit")]
mod imp {
    use std::sync::Arc;
    use std::time::Duration;

    use smithay::backend::input::{
        AbsolutePositionEvent, Event, InputEvent, KeyboardKeyEvent, PointerButtonEvent,
    };
    use smithay::backend::renderer::gles::GlesRenderer;
    use smithay::backend::winit::{self, WinitEvent, WinitEventLoop};
    use smithay::input::keyboard::FilterResult;
    use smithay::input::pointer::{ButtonEvent, MotionEvent};
    use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
    use smithay::reexports::calloop::generic::Generic;
    use smithay::reexports::calloop::{
        EventLoop, Interest, LoopSignal, Mode as CalloopMode, PostAction,
    };
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    use smithay::reexports::wayland_server::Display;
    use smithay::utils::{Point, SERIAL_COUNTER};
    use spiders_runtime::{ControllerCommand, ControllerReport};
    use spiders_shared::ids::OutputId;

    use crate::smithay_adapter::{SmithayAdapter, SmithayOutputDescriptor, SmithaySeatDescriptor};
    use crate::smithay_state::{SmithayClientState, SmithayStateError, SpidersSmithayState};

    #[derive(Debug, thiserror::Error)]
    pub enum SmithayRuntimeError {
        #[error("winit backend init failed: {0}")]
        Winit(String),
        #[error(transparent)]
        State(#[from] SmithayStateError),
        #[error(transparent)]
        Controller(#[from] crate::controller::ControllerCommandError),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayStartupReport {
        pub controller: ControllerReport,
        pub output_name: String,
        pub seat_name: String,
        pub logical_size: (i32, i32),
        pub socket_name: Option<String>,
    }

    #[derive(Debug)]
    pub struct SmithayBootstrap<L, R> {
        pub controller: crate::CompositorController<L, R>,
        pub runtime: SmithayWinitRuntime<'static>,
        pub report: SmithayStartupReport,
    }

    #[derive(Debug)]
    pub struct SmithayWinitRuntime<'a> {
        event_loop: EventLoop<'a, SpidersSmithayState>,
        display_handle: smithay::reexports::wayland_server::DisplayHandle,
        loop_signal: LoopSignal,
        socket_name: String,
        window_size: (i32, i32),
        state: Option<SpidersSmithayState>,
        winit: Option<WinitEventLoop>,
    }

    impl SmithayWinitRuntime<'_> {
        pub fn socket_name(&self) -> &str {
            &self.socket_name
        }

        pub fn display_handle(&self) -> &smithay::reexports::wayland_server::DisplayHandle {
            &self.display_handle
        }

        pub fn loop_signal(&self) -> &LoopSignal {
            &self.loop_signal
        }

        pub fn state(&self) -> &SpidersSmithayState {
            self.state.as_ref().expect("smithay runtime state missing")
        }

        pub fn state_mut(&mut self) -> &mut SpidersSmithayState {
            self.state.as_mut().expect("smithay runtime state missing")
        }

        pub fn run_startup_cycle(&mut self) -> Result<(), SmithayRuntimeError> {
            self.dispatch_winit_events()?;

            let state = self.state.as_mut().ok_or_else(|| {
                SmithayRuntimeError::Winit("smithay runtime state missing".into())
            })?;

            self.event_loop
                .dispatch(Some(Duration::ZERO), state)
                .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

            state
                .display_handle
                .flush_clients()
                .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

            Ok(())
        }

        fn dispatch_winit_events(&mut self) -> Result<(), SmithayRuntimeError> {
            let winit = self
                .winit
                .as_mut()
                .ok_or_else(|| SmithayRuntimeError::Winit("winit event loop missing".into()))?;

            let mut pending_events = Vec::new();
            let status = winit.dispatch_new_events(|event| pending_events.push(event));
            if let smithay::reexports::winit::platform::pump_events::PumpStatus::Exit(_) = status {
                self.loop_signal.stop();
            }

            let mut window_size = self.window_size;
            let state = self.state_mut();
            for event in pending_events {
                handle_winit_event(state, event, &mut window_size)?;
            }
            self.window_size = window_size;

            Ok(())
        }
    }

    fn handle_winit_event(
        state: &mut SpidersSmithayState,
        event: WinitEvent,
        window_size: &mut (i32, i32),
    ) -> Result<(), SmithayRuntimeError> {
        match event {
            WinitEvent::Input(input) => handle_input_event(state, input, window_size),
            WinitEvent::CloseRequested => Ok(()),
            WinitEvent::Resized { size, .. } => {
                *window_size = (size.w, size.h);
                Ok(())
            }
            WinitEvent::Focus(_) | WinitEvent::Redraw => Ok(()),
        }
    }

    fn handle_input_event<I>(
        state: &mut SpidersSmithayState,
        event: InputEvent<I>,
        window_size: &mut (i32, i32),
    ) -> Result<(), SmithayRuntimeError>
    where
        I: smithay::backend::input::InputBackend,
    {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let keyboard = state.seat.get_keyboard().ok_or_else(|| {
                    SmithayRuntimeError::Winit("smithay keyboard capability missing".into())
                })?;
                let serial = SERIAL_COUNTER.next_serial();

                keyboard.input::<(), _>(
                    state,
                    event.key_code(),
                    event.state(),
                    serial,
                    event.time_msec(),
                    |_, _, _| FilterResult::Forward,
                );

                Ok(())
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let pointer = state.seat.get_pointer().ok_or_else(|| {
                    SmithayRuntimeError::Winit("smithay pointer capability missing".into())
                })?;
                let serial = SERIAL_COUNTER.next_serial();
                let location = event.position_transformed((*window_size).into());

                pointer.motion(
                    state,
                    None::<(WlSurface, Point<f64, smithay::utils::Logical>)>,
                    &MotionEvent {
                        location,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(state);

                Ok(())
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = state.seat.get_pointer().ok_or_else(|| {
                    SmithayRuntimeError::Winit("smithay pointer capability missing".into())
                })?;
                let serial = SERIAL_COUNTER.next_serial();

                pointer.button(
                    state,
                    &ButtonEvent {
                        serial,
                        time: event.time_msec(),
                        button: event.button_code(),
                        state: event.state(),
                    },
                );
                pointer.frame(state);

                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn initialize_winit_controller<L, R>(
        runtime_service: spiders_config::service::ConfigRuntimeService<L, R>,
        config: spiders_config::model::Config,
        state: spiders_shared::wm::StateSnapshot,
    ) -> Result<crate::CompositorController<L, R>, SmithayRuntimeError>
    where
        L: spiders_config::loader::LayoutSourceLoader,
        R: spiders_config::runtime::LayoutRuntime,
    {
        crate::CompositorController::initialize(runtime_service, config, state)
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))
    }

    pub fn bootstrap_winit<L, R>(
        runtime_service: spiders_config::service::ConfigRuntimeService<L, R>,
        config: spiders_config::model::Config,
        state: spiders_shared::wm::StateSnapshot,
    ) -> Result<SmithayBootstrap<L, R>, SmithayRuntimeError>
    where
        L: spiders_config::loader::LayoutSourceLoader,
        R: spiders_config::runtime::LayoutRuntime,
    {
        let mut controller = initialize_winit_controller(runtime_service, config, state)?;
        let (runtime, report) = bootstrap_winit_controller(&mut controller)?;

        Ok(SmithayBootstrap {
            controller,
            runtime,
            report,
        })
    }

    pub fn bootstrap_winit_controller<L, R>(
        controller: &mut crate::CompositorController<L, R>,
    ) -> Result<(SmithayWinitRuntime<'static>, SmithayStartupReport), SmithayRuntimeError>
    where
        L: spiders_config::loader::LayoutSourceLoader,
        R: spiders_config::runtime::LayoutRuntime,
    {
        let event_loop = EventLoop::<SpidersSmithayState>::try_new()
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let display =
            Display::new().map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let smithay_state = SpidersSmithayState::new(&display, "smithay-winit")?;
        let socket = smithay_state.bind_auto_socket_source()?;
        let socket_name = socket.socket_name().to_string_lossy().into_owned();

        event_loop
            .handle()
            .insert_source(socket, |client_stream, _, state| {
                let _ = state
                    .display_handle
                    .insert_client(client_stream, Arc::new(SmithayClientState::default()));
            })
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

        event_loop
            .handle()
            .insert_source(
                Generic::new(display, Interest::READ, CalloopMode::Level),
                |_, display, state| {
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }

                    Ok(PostAction::Continue)
                },
            )
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

        let (backend, winit) = winit::init::<GlesRenderer>()
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let size = backend.window_size();

        let seat_name = String::from("smithay-winit");
        let output_name = String::from("smithay-winit-output");
        let output_id = OutputId::from(output_name.as_str());

        let _smithay_output = Output::new(
            output_name.clone(),
            PhysicalProperties {
                size: (size.w, size.h).into(),
                subpixel: Subpixel::Unknown,
                make: "Spiders".into(),
                model: "Winit".into(),
                serial_number: "Bootstrap".into(),
            },
        );
        let _mode = Mode {
            size: (size.w, size.h).into(),
            refresh: 60_000,
        };

        let command = SmithayAdapter::translate_snapshot(
            1,
            vec![SmithayAdapter::translate_seat_descriptor(
                SmithaySeatDescriptor {
                    seat_name: seat_name.clone(),
                    active: true,
                },
            )],
            vec![SmithayAdapter::translate_output_descriptor(
                SmithayOutputDescriptor {
                    output_id: output_id.to_string(),
                    active: true,
                    width: size.w,
                    height: size.h,
                },
            )],
            Vec::new(),
        );

        match command {
            ControllerCommand::DiscoverySnapshot(snapshot) => {
                let _ = (size.w, size.h);
                controller.apply_command(ControllerCommand::DiscoverySnapshot(snapshot))?;
            }
            other => {
                controller.apply_command(other)?;
            }
        }

        let runtime = SmithayWinitRuntime {
            display_handle: smithay_state.display_handle.clone(),
            loop_signal: event_loop.get_signal(),
            event_loop,
            socket_name: socket_name.clone(),
            window_size: (size.w, size.h),
            state: Some(smithay_state),
            winit: Some(winit),
        };

        Ok((
            runtime,
            SmithayStartupReport {
                controller: controller.report(),
                output_name,
                seat_name,
                logical_size: (size.w, size.h),
                socket_name: Some(socket_name),
            },
        ))
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{
    bootstrap_winit, bootstrap_winit_controller, initialize_winit_controller, SmithayBootstrap,
    SmithayRuntimeError, SmithayStartupReport, SmithayWinitRuntime,
};

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, thiserror::Error)]
pub enum SmithayRuntimeError {
    #[error("smithay-winit feature is disabled")]
    Disabled,
}

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmithayStartupReport;
