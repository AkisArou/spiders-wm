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
    use smithay::output::{Mode, Output, PhysicalProperties, Scale, Subpixel};
    use smithay::reexports::calloop::generic::Generic;
    use smithay::reexports::calloop::{
        EventLoop, Interest, LoopSignal, Mode as CalloopMode, PostAction,
    };
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    use smithay::reexports::wayland_server::Display;
    use smithay::utils::{Point, Transform, SERIAL_COUNTER};
    use spiders_runtime::{
        CompositorTopologyState, ControllerCommand, ControllerReport, OutputState, SeatState,
        SurfaceState,
    };
    use spiders_shared::ids::OutputId;
    use spiders_shared::wm::OutputSnapshot;

    use crate::smithay_adapter::{SmithayAdapter, SmithayAdapterEvent, SmithaySeatDescriptor};
    use crate::smithay_state::{
        SmithayClientState, SmithayStateError, SmithayStateSnapshot, SpidersSmithayState,
    };

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

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayRuntimeSnapshot {
        pub socket_name: String,
        pub window_size: (i32, i32),
        pub state: SmithayStateSnapshot,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayBootstrapSnapshot {
        pub runtime: SmithayRuntimeSnapshot,
        pub controller: ControllerReport,
        pub topology: SmithayBootstrapTopologySnapshot,
        pub topology_surface_count: usize,
        pub topology_output_count: usize,
        pub topology_seat_count: usize,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayBootstrapTopologySnapshot {
        pub active_output_id: Option<OutputId>,
        pub active_seat_name: Option<String>,
        pub outputs: Vec<OutputState>,
        pub seats: Vec<SeatState>,
        pub surfaces: Vec<SurfaceState>,
    }

    #[derive(Debug)]
    pub struct SmithayBootstrap<L, R> {
        pub controller: crate::CompositorController<L, R>,
        pub runtime: SmithayWinitRuntime<'static>,
        pub report: SmithayStartupReport,
    }

    impl<L, R> SmithayBootstrap<L, R>
    where
        L: spiders_config::loader::LayoutSourceLoader,
        R: spiders_config::runtime::LayoutRuntime,
    {
        pub fn run_startup_cycle(&mut self) -> Result<(), SmithayRuntimeError> {
            self.runtime.run_startup_cycle()?;

            self.apply_pending_discovery_events()?;
            Ok(())
        }

        pub fn snapshot(&self) -> SmithayBootstrapSnapshot {
            let topology = self.controller.app().topology();
            SmithayBootstrapSnapshot {
                runtime: self.runtime.snapshot(),
                controller: self.controller.report(),
                topology: snapshot_topology(topology),
                topology_surface_count: topology.surfaces.len(),
                topology_output_count: topology.outputs.len(),
                topology_seat_count: topology.seats.len(),
            }
        }

        pub fn apply_pending_discovery_events(&mut self) -> Result<usize, SmithayRuntimeError> {
            let commands = self.runtime.drain_pending_discovery_commands();
            let applied = self.apply_pending_discovery_commands(commands)?;
            self.apply_pending_workspace_actions()?;
            Ok(applied)
        }

        pub fn apply_pending_workspace_actions(&mut self) -> Result<usize, SmithayRuntimeError> {
            let actions = self.runtime.take_workspace_actions();
            let mut applied = 0;

            for action in actions {
                self.controller.apply_ipc_action(&action).map_err(|error| {
                    SmithayRuntimeError::Winit(format!("workspace action failed: {error}"))
                })?;
                refresh_workspace_export_from_controller(
                    &self.controller,
                    self.runtime.state_mut(),
                );
                self.report.controller = self.controller.report();
                applied += 1;
            }

            Ok(applied)
        }

        pub fn apply_adapter_event(
            &mut self,
            event: SmithayAdapterEvent,
        ) -> Result<(), SmithayRuntimeError> {
            self.apply_controller_command(SmithayAdapter::translate_event(event))
        }

        pub fn apply_adapter_events(
            &mut self,
            events: Vec<SmithayAdapterEvent>,
        ) -> Result<usize, SmithayRuntimeError> {
            let commands = events
                .into_iter()
                .map(SmithayAdapter::translate_event)
                .collect();
            self.apply_pending_discovery_commands(commands)
        }

        pub fn apply_adapter_surface_discovery_batch(
            &mut self,
            generation: u64,
            surfaces: Vec<crate::backend::BackendSurfaceSnapshot>,
        ) -> Result<(), SmithayRuntimeError> {
            self.apply_controller_command(SmithayAdapter::translate_snapshot(
                generation,
                Vec::new(),
                Vec::new(),
                surfaces,
            ))
        }

        pub fn apply_tracked_smithay_surface_discovery(
            &mut self,
            generation: u64,
        ) -> Result<(), SmithayRuntimeError> {
            let surfaces = self.runtime.state().backend_surface_snapshots();
            self.apply_adapter_surface_discovery_batch(generation, surfaces)
        }

        pub fn apply_tracked_smithay_discovery_snapshot(
            &mut self,
            generation: u64,
        ) -> Result<(), SmithayRuntimeError> {
            let snapshot = self.runtime.state().backend_topology_snapshot(generation);
            self.apply_controller_command(ControllerCommand::DiscoverySnapshot(snapshot))
        }

        pub fn apply_adapter_discovery_batch(
            &mut self,
            generation: u64,
            seats: Vec<crate::backend::BackendSeatSnapshot>,
            outputs: Vec<crate::backend::BackendOutputSnapshot>,
            surfaces: Vec<crate::backend::BackendSurfaceSnapshot>,
        ) -> Result<(), SmithayRuntimeError> {
            self.apply_controller_command(SmithayAdapter::translate_snapshot(
                generation, seats, outputs, surfaces,
            ))
        }

        pub fn apply_pending_discovery_commands(
            &mut self,
            commands: Vec<ControllerCommand>,
        ) -> Result<usize, SmithayRuntimeError> {
            let mut applied = 0;

            for command in commands {
                self.apply_controller_command(command)?;
                applied += 1;
            }

            Ok(applied)
        }

        fn apply_controller_command(
            &mut self,
            command: ControllerCommand,
        ) -> Result<(), SmithayRuntimeError> {
            self.controller.apply_command(command)?;
            refresh_workspace_export_from_controller(&self.controller, self.runtime.state_mut());
            self.report.controller = self.controller.report();
            Ok(())
        }
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

        pub fn snapshot(&self) -> SmithayRuntimeSnapshot {
            SmithayRuntimeSnapshot {
                socket_name: self.socket_name.clone(),
                window_size: self.window_size,
                state: self.state().snapshot(),
            }
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

        pub fn take_pending_discovery_events(
            &mut self,
        ) -> Vec<crate::backend::BackendDiscoveryEvent> {
            self.state_mut().take_discovery_events()
        }

        pub fn drain_pending_discovery_commands(&mut self) -> Vec<ControllerCommand> {
            self.take_pending_discovery_events()
                .into_iter()
                .map(ControllerCommand::DiscoveryEvent)
                .collect()
        }

        pub fn take_workspace_actions(&mut self) -> Vec<spiders_shared::api::WmAction> {
            self.state_mut().take_workspace_actions()
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
                state.update_active_output_size((size.w.max(0) as u32, size.h.max(0) as u32));
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

    fn snapshot_topology(topology: &CompositorTopologyState) -> SmithayBootstrapTopologySnapshot {
        SmithayBootstrapTopologySnapshot {
            active_output_id: topology.active_output_id.clone(),
            active_seat_name: topology.active_seat_name.clone(),
            outputs: topology.outputs.clone(),
            seats: topology.seats.clone(),
            surfaces: topology.surfaces.clone(),
        }
    }

    fn smithay_output_snapshot(output_name: &str, size: (i32, i32)) -> OutputSnapshot {
        OutputSnapshot {
            id: OutputId::from(output_name),
            name: output_name.into(),
            logical_width: size.0.max(0) as u32,
            logical_height: size.1.max(0) as u32,
            scale: 1,
            transform: spiders_shared::wm::OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        }
    }

    fn initial_winit_discovery_command(
        seat_name: &str,
        output_name: &str,
        size: (i32, i32),
    ) -> ControllerCommand {
        SmithayAdapter::translate_snapshot(
            1,
            vec![SmithayAdapter::translate_seat_descriptor(
                initial_winit_seat_descriptor(seat_name),
            )],
            vec![crate::backend::BackendOutputSnapshot {
                snapshot: smithay_output_snapshot(output_name, size),
                active: true,
            }],
            Vec::new(),
        )
    }

    fn initial_winit_seat_descriptor(seat_name: &str) -> SmithaySeatDescriptor {
        SmithaySeatDescriptor {
            seat_name: seat_name.into(),
            active: true,
        }
    }

    pub fn refresh_workspace_export_from_controller<L, R>(
        controller: &crate::CompositorController<L, R>,
        state: &mut SpidersSmithayState,
    ) where
        L: spiders_config::loader::LayoutSourceLoader,
        R: spiders_config::runtime::LayoutRuntime,
    {
        let snapshot = controller.state_snapshot();
        state.refresh_workspace_state(&snapshot);
        state.refresh_workspace_output_groups();
    }

    pub fn initialize_smithay_workspace_export<L, R>(
        controller: &crate::CompositorController<L, R>,
        state: &mut SpidersSmithayState,
    ) where
        L: spiders_config::loader::LayoutSourceLoader,
        R: spiders_config::runtime::LayoutRuntime,
    {
        refresh_workspace_export_from_controller(controller, state);
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
        let mut smithay_state = SpidersSmithayState::new(&display, "smithay-winit")?;
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
        let smithay_output = Output::new(
            output_name.clone(),
            PhysicalProperties {
                size: (size.w, size.h).into(),
                subpixel: Subpixel::Unknown,
                make: "Spiders".into(),
                model: "Winit".into(),
                serial_number: "Bootstrap".into(),
            },
        );
        let mode = Mode {
            size: (size.w, size.h).into(),
            refresh: 60_000,
        };

        smithay_output.change_current_state(
            Some(mode),
            Some(Transform::Normal),
            Some(Scale::Integer(1)),
            Some((0, 0).into()),
        );
        smithay_output.set_preferred(mode);
        let _global =
            smithay_output.create_global::<SpidersSmithayState>(&smithay_state.display_handle);

        smithay_state.register_smithay_output(
            OutputId::from(output_name.as_str()),
            smithay_output,
            Some((size.w.max(0) as u32, size.h.max(0) as u32)),
            true,
        );

        let command = initial_winit_discovery_command(&seat_name, &output_name, (size.w, size.h));

        match command {
            ControllerCommand::DiscoverySnapshot(snapshot) => {
                let _ = (size.w, size.h);
                controller.apply_command(ControllerCommand::DiscoverySnapshot(snapshot))?;
            }
            other => {
                controller.apply_command(other)?;
            }
        }

        initialize_smithay_workspace_export(controller, &mut smithay_state);

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

    #[cfg(test)]
    mod tests {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        use spiders_config::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
        use spiders_config::model::{Config, LayoutDefinition};
        use spiders_config::runtime::BoaLayoutRuntime;
        use spiders_config::service::ConfigRuntimeService;
        use spiders_runtime::{
            ControllerPhase, LayerExclusiveZone, LayerKeyboardInteractivity, LayerSurfaceMetadata,
            LayerSurfaceTier, SurfaceRole,
        };
        use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
        use spiders_shared::wm::{
            LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot,
        };

        use super::*;

        type TestLoader = RuntimeProjectLayoutSourceLoader;
        type TestLayoutRuntime = BoaLayoutRuntime<TestLoader>;
        type TestBootstrap = SmithayBootstrap<TestLoader, TestLayoutRuntime>;

        fn test_state_snapshot() -> StateSnapshot {
            StateSnapshot {
                focused_window_id: None,
                current_output_id: Some(OutputId::from("out-1")),
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
                outputs: vec![OutputSnapshot {
                    id: OutputId::from("out-1"),
                    name: "HDMI-A-1".into(),
                    logical_width: 800,
                    logical_height: 600,
                    scale: 1,
                    transform: OutputTransform::Normal,
                    enabled: true,
                    current_workspace_id: Some(WorkspaceId::from("ws-1")),
                }],
                workspaces: vec![WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-1"),
                    name: "1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["1".into()],
                    focused: true,
                    visible: true,
                    effective_layout: Some(LayoutRef {
                        name: "master-stack".into(),
                    }),
                }],
                windows: vec![],
                visible_window_ids: vec![],
                tag_names: vec!["1".into()],
            }
        }

        fn test_config() -> Config {
            Config {
                layouts: vec![LayoutDefinition {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                }],
                ..Config::default()
            }
        }

        fn test_runtime_service() -> ConfigRuntimeService<
            RuntimeProjectLayoutSourceLoader,
            BoaLayoutRuntime<RuntimeProjectLayoutSourceLoader>,
        > {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let runtime_root = std::env::temp_dir().join(format!(
                "spiders-smithay-runtime-test-{}-{}",
                std::process::id(),
                unique
            ));

            fs::create_dir_all(runtime_root.join("layouts")).unwrap();
            fs::write(
                runtime_root.join("layouts/master-stack.js"),
                "ctx => ({ type: 'workspace', children: [] })",
            )
            .unwrap();

            let loader =
                RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
            let runtime = BoaLayoutRuntime::with_loader(loader.clone());
            ConfigRuntimeService::new(loader, runtime)
        }

        fn test_runtime(socket_name: &str) -> SmithayWinitRuntime<'static> {
            let event_loop = EventLoop::<SpidersSmithayState>::try_new().unwrap();
            let display = Display::new().unwrap();
            let state = SpidersSmithayState::new(&display, "smithay-test-seat").unwrap();

            SmithayWinitRuntime {
                display_handle: state.display_handle.clone(),
                loop_signal: event_loop.get_signal(),
                event_loop,
                socket_name: socket_name.into(),
                window_size: (1280, 720),
                state: Some(state),
                winit: None,
            }
        }

        fn test_bootstrap(socket_name: &str) -> TestBootstrap {
            test_bootstrap_with_state(socket_name, test_state_snapshot())
        }

        fn test_bootstrap_with_state(socket_name: &str, state: StateSnapshot) -> TestBootstrap {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime(socket_name);
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some(socket_name.into()),
            };

            SmithayBootstrap {
                controller,
                runtime,
                report,
            }
        }

        fn assert_topology_matches_known_surfaces(snapshot: &SmithayBootstrapSnapshot) {
            let runtime_state = &snapshot.runtime.state;
            let topology = &snapshot.topology;

            assert_eq!(
                topology.surfaces.len(),
                runtime_state.known_surfaces.all.len()
            );

            for toplevel in &runtime_state.known_surfaces.toplevels {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == toplevel.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Window);
                assert_eq!(surface.window_id.as_ref(), Some(&toplevel.window_id));
                assert_eq!(surface.parent_surface_id, None);
                assert!(surface.mapped);
                assert_eq!(surface.layer_metadata, None);
                assert!(
                    toplevel.requests.last_resize_serial.is_none()
                        || toplevel.requests.last_resize_edge.is_some()
                );
            }

            for popup in &runtime_state.known_surfaces.popups {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == popup.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Popup);
                assert!(surface.window_id.is_none());
                assert!(surface.mapped);
                assert!(popup.configure.pending_configure_count <= 1);
                if popup.configure.grab_requested {
                    assert!(popup.configure.last_grab_serial.is_some());
                }

                match &popup.parent {
                    crate::smithay_state::SmithayPopupParentSnapshot::Resolved {
                        surface_id,
                        ..
                    } => {
                        assert_eq!(
                            surface.parent_surface_id.as_deref(),
                            Some(surface_id.as_str())
                        );
                    }
                    crate::smithay_state::SmithayPopupParentSnapshot::Unresolved => {
                        assert_eq!(
                            surface.parent_surface_id.as_deref(),
                            Some(format!("unresolved-parent-{}", popup.surface_id).as_str())
                        );
                    }
                }
            }

            for unmanaged in &runtime_state.known_surfaces.unmanaged {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == unmanaged.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Unmanaged);
                assert!(surface.window_id.is_none());
                assert_eq!(surface.parent_surface_id, None);
                assert!(surface.mapped);
            }

            for layer in &runtime_state.known_surfaces.layers {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == layer.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Layer);
                assert_eq!(surface.output_id, layer.output_id);
                assert_eq!(surface.layer_metadata.as_ref(), Some(&layer.metadata));
                assert!(surface.window_id.is_none());
                assert_eq!(surface.parent_surface_id, None);
                assert!(surface.mapped);
            }
        }

        fn assert_output_summary_matches_topology(snapshot: &SmithayBootstrapSnapshot) {
            let runtime_state = &snapshot.runtime.state;
            let topology = &snapshot.topology;

            assert!(topology.outputs.len() >= runtime_state.outputs.known_output_ids.len());
            for output_id in &runtime_state.outputs.known_output_ids {
                assert!(topology
                    .outputs
                    .iter()
                    .any(|output| output.snapshot.id == *output_id));
            }
            if let Some(active_output_id) = runtime_state.outputs.active_output_id.as_ref() {
                assert_eq!(topology.active_output_id.as_ref(), Some(active_output_id));
            }
            assert_eq!(
                runtime_state.outputs.mapped_surface_count,
                topology
                    .surfaces
                    .iter()
                    .filter(|surface| surface.mapped)
                    .count()
            );

            let topology_active_output_attached_surface_count = runtime_state
                .outputs
                .active_output_id
                .as_ref()
                .map(|active_output_id| {
                    topology
                        .surfaces
                        .iter()
                        .filter(|surface| {
                            surface.mapped && surface.output_id.as_ref() == Some(active_output_id)
                        })
                        .count()
                })
                .unwrap_or(0);
            assert_eq!(
                runtime_state.outputs.active_output_attached_surface_count,
                topology_active_output_attached_surface_count
            );
        }

        fn assert_seat_summary_matches_topology(snapshot: &SmithayBootstrapSnapshot) {
            let runtime_state = &snapshot.runtime.state;
            let topology = &snapshot.topology;

            if let Some(seat) = topology
                .seats
                .iter()
                .find(|seat| seat.name == runtime_state.seat.name)
            {
                assert_eq!(
                    topology.active_seat_name.as_deref(),
                    Some(seat.name.as_str())
                );
                assert!(seat.active);
                assert_eq!(runtime_state.seat.focused_window_id, seat.focused_window_id);
                if let Some(focused_output_id) = runtime_state.seat.focused_output_id.as_ref() {
                    assert_eq!(seat.focused_output_id.as_ref(), Some(focused_output_id));
                }
            }
        }

        #[test]
        fn runtime_snapshot_exposes_state_snapshot() {
            let mut runtime = test_runtime("wayland-test-1");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Unmanaged {
                    surface_id: "wl-surface-1".into(),
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.socket_name, "wayland-test-1");
            assert_eq!(snapshot.window_size, (1280, 720));
            assert_eq!(snapshot.state.seat_name, "smithay-test-seat");
            assert_eq!(snapshot.state.seat.name, "smithay-test-seat");
            assert!(snapshot.state.seat.has_keyboard);
            assert!(snapshot.state.seat.has_pointer);
            assert!(!snapshot.state.seat.has_touch);
            assert!(snapshot.state.seat.focused_surface_id.is_none());
            assert!(snapshot.state.seat.focused_surface_role.is_none());
            assert!(snapshot.state.seat.focused_window_id.is_none());
            assert!(snapshot.state.seat.focused_output_id.is_none());
            assert_eq!(snapshot.state.seat.cursor_image, "default");
            assert!(snapshot.state.seat.cursor_surface_id.is_none());
            assert!(snapshot.state.outputs.known_output_ids.is_empty());
            assert!(snapshot.state.outputs.active_output_id.is_none());
            assert_eq!(snapshot.state.outputs.layer_surface_output_count, 0);
            assert_eq!(
                snapshot.state.outputs.active_output_attached_surface_count,
                0
            );
            assert_eq!(snapshot.state.outputs.mapped_surface_count, 1);
            assert_eq!(snapshot.state.tracked_surface_count, 1);
            assert_eq!(snapshot.state.role_counts.unmanaged, 1);
            assert_eq!(snapshot.state.known_surfaces.unmanaged.len(), 1);
            assert_eq!(snapshot.state.clipboard_selection.target, "clipboard");
            assert!(snapshot.state.selection_protocols.data_device);
            assert!(snapshot.state.selection_protocols.primary_selection);
            assert!(snapshot.state.selection_protocols.wlr_data_control);
            assert!(snapshot.state.selection_protocols.ext_data_control);
            assert!(snapshot.state.clipboard_selection.selection.is_none());
            assert!(snapshot
                .state
                .clipboard_selection
                .focused_client_id
                .is_none());
            assert_eq!(snapshot.state.primary_selection.target, "primary");
            assert!(snapshot.state.primary_selection.selection.is_none());
            assert!(snapshot.state.primary_selection.focused_client_id.is_none());
        }

        #[test]
        fn bootstrap_snapshot_matches_runtime_snapshot() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime("wayland-test-2");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-2".into()),
            };
            let bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.runtime, bootstrap.runtime.snapshot());
            assert_eq!(snapshot.runtime.socket_name, "wayland-test-2");
            assert_eq!(snapshot.controller, bootstrap.controller.report());
            assert_eq!(bootstrap.report.seat_name, "smithay-test-seat");
            assert_eq!(bootstrap.controller.phase(), ControllerPhase::Pending);
        }

        #[test]
        fn bootstrap_applies_pending_discovery_events_to_controller() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let mut runtime = test_runtime("wayland-test-3");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-601".into(),
                    window_id: spiders_shared::ids::WindowId::from("smithay-window-601"),
                    output_id: None,
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-3".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            let applied = bootstrap.apply_pending_discovery_events().unwrap();

            let snapshot = bootstrap.snapshot();
            assert_eq!(applied, 1);
            assert_eq!(snapshot.runtime.state.pending_discovery_event_count, 0);
            assert_eq!(snapshot.runtime.state.known_surfaces.toplevels.len(), 1);
            assert_eq!(snapshot.topology_surface_count, 1);
            assert_eq!(bootstrap.controller.phase(), ControllerPhase::Running);
            let surface = bootstrap
                .controller
                .app()
                .session()
                .topology()
                .surface("wl-surface-601")
                .unwrap();
            assert_eq!(surface.id, "wl-surface-601");
            assert_eq!(
                surface.window_id,
                Some(spiders_shared::ids::WindowId::from("smithay-window-601"))
            );
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].configure,
                crate::smithay_state::SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: false,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 0,
                }
            );
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].metadata,
                crate::smithay_state::SmithayXdgToplevelMetadataSnapshot {
                    title: None,
                    app_id: None,
                    parent_surface_id: None,
                    min_size: None,
                    max_size: None,
                    window_geometry: None,
                }
            );
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].requests,
                crate::smithay_state::SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: None,
                    last_resize_serial: None,
                    last_resize_edge: None,
                    last_window_menu_serial: None,
                    last_window_menu_location: None,
                    minimize_requested: false,
                    last_request_kind: None,
                    request_count: 0,
                }
            );
            assert_topology_matches_known_surfaces(&snapshot);
            assert_output_summary_matches_topology(&snapshot);
            assert_seat_summary_matches_topology(&snapshot);
        }

        #[test]
        fn runtime_drains_pending_discovery_events_as_controller_commands() {
            let mut runtime = test_runtime("wayland-test-discovery-drain");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);
            runtime
                .state_mut()
                .activate_output_id(OutputId::from("out-2"));
            runtime
                .state_mut()
                .remove_output_id(&OutputId::from("out-2"));

            let commands = runtime.drain_pending_discovery_commands();

            assert_eq!(commands.len(), 2);
            assert!(matches!(
                &commands[0],
                ControllerCommand::DiscoveryEvent(
                    crate::backend::BackendDiscoveryEvent::OutputActivated { output_id }
                ) if output_id == &OutputId::from("out-2")
            ));
            assert!(matches!(
                &commands[1],
                ControllerCommand::DiscoveryEvent(
                    crate::backend::BackendDiscoveryEvent::OutputLost { output_id }
                ) if output_id == &OutputId::from("out-2")
            ));
        }

        #[test]
        fn bootstrap_applies_adapter_output_lifecycle_events_to_controller() {
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let mut bootstrap =
                test_bootstrap_with_state("wayland-test-adapter-output-lifecycle", state);

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::OutputActivated {
                    output_id: "out-2".into(),
                })
                .unwrap();
            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-2"))
            );

            bootstrap
                .apply_adapter_surface_discovery_batch(
                    1,
                    vec![crate::backend::BackendSurfaceSnapshot::Layer {
                        surface_id: "wl-adapter-output-layer-1".into(),
                        output_id: OutputId::from("out-2"),
                        metadata: LayerSurfaceMetadata {
                            namespace: "panel".into(),
                            tier: LayerSurfaceTier::Top,
                            keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                            exclusive_zone: LayerExclusiveZone::Exclusive(10),
                        },
                    }],
                )
                .unwrap();

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::OutputLost {
                    output_id: "out-2".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(snapshot
                .topology
                .outputs
                .iter()
                .all(|output| output.snapshot.id != OutputId::from("out-2")));
            assert_eq!(snapshot.topology.active_output_id, None);
            assert_eq!(
                snapshot
                    .topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == "wl-adapter-output-layer-1")
                    .unwrap()
                    .output_id,
                None
            );
        }

        #[test]
        fn bootstrap_applies_adapter_seat_lifecycle_and_focus_events_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-seat-lifecycle");

            bootstrap
                .apply_adapter_discovery_batch(
                    1,
                    vec![crate::backend::BackendSeatSnapshot {
                        seat_name: "seat-adapter".into(),
                        active: true,
                    }],
                    Vec::new(),
                    Vec::new(),
                )
                .unwrap();
            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SeatFocusChanged {
                    seat_name: "seat-adapter".into(),
                    window_id: Some("w1".into()),
                    output_id: Some("out-1".into()),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let seat = snapshot
                .topology
                .seats
                .iter()
                .find(|seat| seat.name == "seat-adapter")
                .unwrap();
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("seat-adapter")
            );
            assert_eq!(seat.focused_window_id, Some(WindowId::from("w1")));
            assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SeatLost {
                    seat_name: "seat-adapter".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(snapshot
                .topology
                .seats
                .iter()
                .all(|seat| seat.name != "seat-adapter"));
            assert_eq!(snapshot.topology.active_seat_name, None);
        }

        #[test]
        fn bootstrap_applies_adapter_output_discovery_batch_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-output-discovery");

            bootstrap
                .apply_adapter_discovery_batch(
                    1,
                    Vec::new(),
                    vec![crate::backend::BackendOutputSnapshot {
                        snapshot: OutputSnapshot {
                            id: OutputId::from("out-3"),
                            name: "DP-2".into(),
                            logical_width: 3440,
                            logical_height: 1440,
                            scale: 1,
                            transform: OutputTransform::Normal,
                            enabled: true,
                            current_workspace_id: None,
                        },
                        active: true,
                    }],
                    Vec::new(),
                )
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(snapshot
                .topology
                .outputs
                .iter()
                .any(|output| output.snapshot.id == OutputId::from("out-3")));
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-3"))
            );
        }

        #[test]
        fn bootstrap_applies_adapter_output_snapshot_event_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-output-event");

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::OutputSnapshot {
                    output_id: "out-9".into(),
                    active: true,
                    width: 3840,
                    height: 2160,
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let output = snapshot
                .topology
                .outputs
                .iter()
                .find(|output| output.snapshot.id == OutputId::from("out-9"))
                .unwrap();
            assert_eq!(output.snapshot.name, "out-9");
            assert_eq!(output.snapshot.logical_width, 3840);
            assert_eq!(output.snapshot.logical_height, 2160);
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-9"))
            );
        }

        #[test]
        fn smithay_initial_winit_discovery_command_uses_typed_output_snapshot() {
            let command = super::initial_winit_discovery_command(
                "smithay-winit",
                "smithay-winit-output",
                (1280, 720),
            );

            let ControllerCommand::DiscoverySnapshot(snapshot) = command else {
                panic!("expected discovery snapshot command");
            };

            assert_eq!(snapshot.seats.len(), 1);
            assert_eq!(snapshot.outputs.len(), 1);
            assert_eq!(
                snapshot.outputs[0].snapshot.id,
                OutputId::from("smithay-winit-output")
            );
            assert_eq!(snapshot.outputs[0].snapshot.name, "smithay-winit-output");
            assert_eq!(snapshot.outputs[0].snapshot.logical_width, 1280);
            assert_eq!(snapshot.outputs[0].snapshot.logical_height, 720);
            assert!(snapshot.outputs[0].active);
        }

        #[test]
        fn smithay_initial_winit_seat_descriptor_marks_active_seat() {
            let descriptor = super::initial_winit_seat_descriptor("smithay-winit");

            assert_eq!(descriptor.seat_name, "smithay-winit");
            assert!(descriptor.active);
        }

        #[test]
        fn smithay_output_snapshot_matches_state_output_registration_metadata() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "smithay-winit").unwrap();
            let output = super::smithay_output_snapshot("smithay-winit-output", (1280, 720));
            let smithay_output = Output::new(
                output.name.clone(),
                PhysicalProperties {
                    size: (1280, 720).into(),
                    subpixel: Subpixel::Unknown,
                    make: "Spiders".into(),
                    model: "Winit".into(),
                    serial_number: "Test".into(),
                },
            );

            state.register_smithay_output(
                output.id.clone(),
                smithay_output,
                Some((output.logical_width, output.logical_height)),
                true,
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.outputs.known_outputs.len(), 1);
            assert_eq!(snapshot.outputs.known_outputs[0].id, output.id);
            assert_eq!(snapshot.outputs.known_outputs[0].name, output.name);
            assert_eq!(
                snapshot.outputs.known_outputs[0].logical_width,
                Some(output.logical_width)
            );
            assert_eq!(
                snapshot.outputs.known_outputs[0].logical_height,
                Some(output.logical_height)
            );
            assert_eq!(
                snapshot.outputs.active_output_id,
                Some(OutputId::from("smithay-winit-output"))
            );
        }

        #[test]
        fn smithay_state_extracts_backend_surface_snapshots_from_tracked_surfaces() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_id(OutputId::from("out-1"), true);
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Window {
                surface_id: "wl-window-extract-1".into(),
                window_id: WindowId::from("w1"),
                output_id: None,
            });
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-extract-1".into(),
                output_id: Some(OutputId::from("out-1")),
                parent_surface_id: "wl-window-extract-1".into(),
            });
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-extract-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(8),
                },
            });
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-unmanaged-extract-1".into(),
            });

            let snapshots = state.backend_surface_snapshots();
            assert_eq!(snapshots.len(), 4);
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Window { surface_id, .. } if surface_id == "wl-window-extract-1")
            }));
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Popup { surface_id, .. } if surface_id == "wl-popup-extract-1")
            }));
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Layer { surface_id, .. } if surface_id == "wl-layer-extract-1")
            }));
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Unmanaged { surface_id } if surface_id == "wl-unmanaged-extract-1")
            }));
        }

        #[test]
        fn bootstrap_applies_tracked_smithay_surface_discovery_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-tracked-surface-discovery");
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-bootstrap-window-1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: None,
                },
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-bootstrap-popup-1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    parent_surface_id: "wl-bootstrap-window-1".into(),
                },
            );

            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_surface_discovery(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(snapshot
                .topology
                .surfaces
                .iter()
                .any(|surface| surface.id == "wl-bootstrap-window-1"
                    && surface.role == SurfaceRole::Window));
            assert!(snapshot
                .topology
                .surfaces
                .iter()
                .any(|surface| surface.id == "wl-bootstrap-popup-1"
                    && surface.role == SurfaceRole::Popup));
        }

        #[test]
        fn smithay_state_extracts_backend_topology_snapshot_from_known_state() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_snapshot(
                OutputId::from("out-topology-1"),
                "DP-1",
                Some((2560, 1440)),
                true,
            );
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Window {
                surface_id: "wl-topology-window-1".into(),
                window_id: WindowId::from("w1"),
                output_id: None,
            });

            let snapshot = state.backend_topology_snapshot(7);
            assert_eq!(snapshot.source, crate::backend::BackendSource::Smithay);
            assert_eq!(snapshot.generation, 7);
            assert_eq!(snapshot.seats.len(), 1);
            assert_eq!(snapshot.outputs.len(), 1);
            assert_eq!(
                snapshot.outputs[0].snapshot.id,
                OutputId::from("out-topology-1")
            );
            assert_eq!(snapshot.outputs[0].snapshot.name, "DP-1");
            assert!(snapshot.outputs[0].active);
            assert_eq!(snapshot.surfaces.len(), 1);
        }

        #[test]
        fn bootstrap_applies_tracked_smithay_discovery_snapshot_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-tracked-discovery-snapshot");
            bootstrap.runtime.state_mut().register_output_snapshot(
                OutputId::from("out-snapshot-1"),
                "HDMI-A-1",
                Some((1920, 1080)),
                true,
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-snapshot-window-1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: Some(OutputId::from("out-snapshot-1")),
                },
            );

            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(9)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(snapshot
                .topology
                .outputs
                .iter()
                .any(|output| output.snapshot.id == OutputId::from("out-snapshot-1")));
            assert!(snapshot
                .topology
                .surfaces
                .iter()
                .any(|surface| surface.id == "wl-snapshot-window-1"
                    && surface.role == SurfaceRole::Window));
            assert_eq!(
                snapshot
                    .controller
                    .backend
                    .as_ref()
                    .and_then(|backend| backend.last_generation),
                Some(9)
            );
        }

        #[test]
        fn bootstrap_applies_adapter_surface_unmap_and_loss_events_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-surface-lifecycle");

            bootstrap
                .apply_adapter_surface_discovery_batch(
                    1,
                    vec![
                        crate::backend::BackendSurfaceSnapshot::Window {
                            surface_id: "wl-adapter-window-1".into(),
                            window_id: WindowId::from("w1"),
                            output_id: Some(OutputId::from("out-1")),
                        },
                        crate::backend::BackendSurfaceSnapshot::Popup {
                            surface_id: "wl-adapter-popup-1".into(),
                            output_id: Some(OutputId::from("out-1")),
                            parent_surface_id: "wl-adapter-window-1".into(),
                        },
                    ],
                )
                .unwrap();

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SurfaceUnmapped {
                    surface_id: "wl-adapter-window-1".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let window = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-adapter-window-1")
                .unwrap();
            let popup = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-adapter-popup-1")
                .unwrap();
            assert!(!window.mapped);
            assert!(!popup.mapped);

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SurfaceLost {
                    surface_id: "wl-adapter-window-1".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(snapshot.topology.surfaces.iter().all(|surface| {
                surface.id != "wl-adapter-window-1" && surface.id != "wl-adapter-popup-1"
            }));
        }

        #[test]
        fn bootstrap_applies_batched_adapter_lifecycle_events_to_controller() {
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let mut bootstrap = test_bootstrap_with_state("wayland-test-adapter-batch", state);

            bootstrap
                .apply_adapter_surface_discovery_batch(
                    1,
                    vec![crate::backend::BackendSurfaceSnapshot::Window {
                        surface_id: "wl-batch-window-1".into(),
                        window_id: WindowId::from("w1"),
                        output_id: Some(OutputId::from("out-2")),
                    }],
                )
                .unwrap();

            let applied = bootstrap
                .apply_adapter_events(vec![
                    SmithayAdapterEvent::Seat {
                        seat_name: "seat-batch".into(),
                        active: true,
                    },
                    SmithayAdapterEvent::SeatFocusChanged {
                        seat_name: "seat-batch".into(),
                        window_id: Some("w1".into()),
                        output_id: Some("out-2".into()),
                    },
                    SmithayAdapterEvent::OutputActivated {
                        output_id: "out-2".into(),
                    },
                    SmithayAdapterEvent::SurfaceUnmapped {
                        surface_id: "wl-batch-window-1".into(),
                    },
                ])
                .unwrap();

            assert_eq!(applied, 4);

            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("seat-batch")
            );
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-2"))
            );
            let seat = snapshot
                .topology
                .seats
                .iter()
                .find(|seat| seat.name == "seat-batch")
                .unwrap();
            assert_eq!(seat.focused_window_id, Some(WindowId::from("w1")));
            assert_eq!(seat.focused_output_id, Some(OutputId::from("out-2")));
            let surface = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-batch-window-1")
                .unwrap();
            assert!(!surface.mapped);
        }

        #[test]
        fn bootstrap_snapshot_exposes_rich_topology_for_mixed_surface_roles() {
            let mut bootstrap = test_bootstrap("wayland-test-5");

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-701".into(),
                    window_id: WindowId::from("smithay-window-701"),
                    output_id: None,
                },
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-surface-702".into(),
                    output_id: None,
                    parent_surface_id: "wl-surface-701".into(),
                },
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Unmanaged {
                    surface_id: "wl-surface-703".into(),
                },
            );
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-surface-704".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.topology_surface_count, 4);
            assert_eq!(snapshot.topology.surfaces.len(), 4);
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("smithay-test-seat")
            );
            assert_topology_matches_known_surfaces(&snapshot);
            assert_output_summary_matches_topology(&snapshot);
            assert_seat_summary_matches_topology(&snapshot);

            let popup = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-702")
                .unwrap();
            assert_eq!(popup.parent_surface_id.as_deref(), Some("wl-surface-701"));
            assert_eq!(popup.role, SurfaceRole::Popup);

            let layer = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-704")
                .unwrap();
            assert_eq!(layer.role, SurfaceRole::Layer);
            assert_eq!(layer.output_id, Some(OutputId::from("out-1")));
            assert_eq!(
                layer.layer_metadata,
                Some(LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                })
            );
        }

        #[test]
        fn runtime_snapshot_exposes_known_layer_surface_output_attachment() {
            let mut runtime = test_runtime("wayland-test-layer-1");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "background".into(),
                        tier: LayerSurfaceTier::Background,
                        keyboard_interactivity: LayerKeyboardInteractivity::None,
                        exclusive_zone: LayerExclusiveZone::Neutral,
                    },
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.role_counts.layer, 1);
            assert_eq!(snapshot.state.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].metadata,
                LayerSurfaceMetadata {
                    namespace: "background".into(),
                    tier: LayerSurfaceTier::Background,
                    keyboard_interactivity: LayerKeyboardInteractivity::None,
                    exclusive_zone: LayerExclusiveZone::Neutral,
                }
            );
            assert_eq!(
                snapshot.state.known_surfaces.layers[0]
                    .configure
                    .last_acked_serial,
                None
            );
            assert_eq!(
                snapshot.state.known_surfaces.layers[0]
                    .configure
                    .last_configured_size,
                None
            );
        }

        #[test]
        fn runtime_snapshot_exposes_layer_configure_inspection() {
            let mut runtime = test_runtime("wayland-test-layer-configure-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-configure-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            runtime.state_mut().set_test_layer_configure_snapshot(
                "wl-layer-configure-1",
                crate::smithay_state::SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(99),
                    pending_configure_count: 0,
                    last_configured_size: Some((1280, 36)),
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].configure,
                crate::smithay_state::SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(99),
                    pending_configure_count: 0,
                    last_configured_size: Some((1280, 36)),
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_layer_pending_configure_counts() {
            let mut runtime = test_runtime("wayland-test-layer-configure-2");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-configure-2".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(18),
                    },
                },
            );
            runtime
                .state_mut()
                .record_test_layer_configure_sent("wl-layer-configure-2", Some((1024, 30)));
            runtime
                .state_mut()
                .record_test_layer_configure_sent("wl-layer-configure-2", Some((1024, 32)));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].configure,
                crate::smithay_state::SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 2,
                    last_configured_size: Some((1024, 32)),
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_explicit_layer_parented_popup_tracking() {
            let mut runtime = test_runtime("wayland-test-layer-popup-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-parent-1".into(),
                    output_id: OutputId::from("out-7"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(10),
                    },
                },
            );
            let _ = runtime.state_mut().take_discovery_events();
            runtime
                .state_mut()
                .track_layer_popup_surface_for_test("wl-layer-parent-1", "wl-popup-child-1");

            let snapshot = runtime.snapshot();
            let popup = snapshot
                .state
                .known_surfaces
                .popups
                .iter()
                .find(|popup| popup.surface_id == "wl-popup-child-1")
                .unwrap();
            assert_eq!(
                popup.parent,
                crate::smithay_state::SmithayPopupParentSnapshot::Resolved {
                    surface_id: "wl-layer-parent-1".into(),
                    window_id: None,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_popup_pending_configure_counts() {
            let mut runtime = test_runtime("wayland-test-popup-configure-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-configure-1".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-configure-1".into(),
                },
            );
            runtime.state_mut().record_test_xdg_popup_configure_sent(
                "wl-popup-configure-1",
                Some(31),
                true,
                (12, 14, 240, 160),
            );
            runtime.state_mut().record_test_xdg_popup_configure_sent(
                "wl-popup-configure-1",
                Some(32),
                true,
                (12, 14, 260, 180),
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 2,
                    last_reposition_token: Some(32),
                    reactive: true,
                    geometry: (12, 14, 260, 180),
                    last_grab_serial: None,
                    grab_requested: false,
                    last_request_kind: Some("reposition".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_initial_popup_pending_configure() {
            let mut runtime = test_runtime("wayland-test-popup-configure-init");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-configure-init".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-configure-init".into(),
                },
            );
            runtime
                .state_mut()
                .record_test_initial_xdg_popup_configure_sent(
                    "wl-popup-configure-init",
                    false,
                    (6, 8, 190, 120),
                );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 1,
                    last_reposition_token: None,
                    reactive: false,
                    geometry: (6, 8, 190, 120),
                    last_grab_serial: None,
                    grab_requested: false,
                    last_request_kind: None,
                    request_count: 0,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_popup_request_sequence() {
            let mut runtime = test_runtime("wayland-test-popup-request-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-request-1".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-request-1".into(),
                },
            );
            runtime.state_mut().record_test_xdg_popup_request(
                "wl-popup-request-1",
                "grab",
                |snapshot| {
                    snapshot.last_grab_serial = Some(51);
                    snapshot.grab_requested = true;
                },
            );
            runtime.state_mut().record_test_xdg_popup_request(
                "wl-popup-request-1",
                "reposition",
                |snapshot| {
                    snapshot.last_reposition_token = Some(52);
                    snapshot.reactive = true;
                    snapshot.geometry = (8, 9, 180, 120);
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 0,
                    last_reposition_token: Some(52),
                    reactive: true,
                    geometry: (8, 9, 180, 120),
                    last_grab_serial: Some(51),
                    grab_requested: true,
                    last_request_kind: Some("reposition".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_toplevel_pending_configure_counts() {
            let mut runtime = test_runtime("wayland-test-toplevel-configure-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-toplevel-configure-1".into(),
                    window_id: WindowId::from("smithay-window-top-1"),
                    output_id: None,
                },
            );
            runtime.state_mut().record_test_xdg_toplevel_configure_sent(
                "wl-toplevel-configure-1",
                true,
                false,
                true,
            );
            runtime.state_mut().record_test_xdg_toplevel_configure_sent(
                "wl-toplevel-configure-1",
                true,
                false,
                false,
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.toplevels[0].configure,
                crate::smithay_state::SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: true,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 2,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_initial_toplevel_pending_configure() {
            let mut runtime = test_runtime("wayland-test-toplevel-configure-init");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-toplevel-configure-init".into(),
                    window_id: WindowId::from("smithay-window-top-init"),
                    output_id: None,
                },
            );
            runtime.state_mut().record_test_xdg_toplevel_configure_sent(
                "wl-toplevel-configure-init",
                true,
                false,
                false,
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.toplevels[0].configure,
                crate::smithay_state::SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: true,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 1,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_toplevel_request_sequence() {
            let mut runtime = test_runtime("wayland-test-toplevel-request-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-toplevel-request-1".into(),
                    window_id: WindowId::from("smithay-window-request-1"),
                    output_id: None,
                },
            );
            runtime.state_mut().set_test_toplevel_request_snapshot(
                "wl-toplevel-request-1",
                crate::smithay_state::SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: Some(41),
                    last_resize_serial: None,
                    last_resize_edge: None,
                    last_window_menu_serial: None,
                    last_window_menu_location: None,
                    minimize_requested: true,
                    last_request_kind: Some("minimize".into()),
                    request_count: 2,
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.known_surfaces.toplevels[0].requests,
                crate::smithay_state::SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: Some(41),
                    last_resize_serial: None,
                    last_resize_edge: None,
                    last_window_menu_serial: None,
                    last_window_menu_location: None,
                    minimize_requested: true,
                    last_request_kind: Some("minimize".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn smithay_bootstrap_preserves_layer_keyboard_and_exclusive_zone_metadata() {
            let mut bootstrap = test_bootstrap("wayland-test-layer-meta-1");
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-meta-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "lockscreen".into(),
                        tier: LayerSurfaceTier::Overlay,
                        keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                        exclusive_zone: LayerExclusiveZone::DontCare,
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let layer = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-layer-meta-1")
                .unwrap();
            assert_eq!(
                layer.layer_metadata,
                Some(LayerSurfaceMetadata {
                    namespace: "lockscreen".into(),
                    tier: LayerSurfaceTier::Overlay,
                    keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                    exclusive_zone: LayerExclusiveZone::DontCare,
                })
            );
            assert_topology_matches_known_surfaces(&snapshot);
            assert_output_summary_matches_topology(&snapshot);
            assert_seat_summary_matches_topology(&snapshot);
        }

        #[test]
        fn bootstrap_unmaps_and_remaps_layer_surface_without_losing_output_attachment() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let mut runtime = test_runtime("wayland-test-layer-2");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-2".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-layer-2".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_unmap("wl-layer-2");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let unmapped = bootstrap.snapshot();
            let layer = unmapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-layer-2")
                .unwrap();
            assert_eq!(layer.role, SurfaceRole::Layer);
            assert_eq!(layer.output_id, Some(OutputId::from("out-1")));
            assert!(!layer.mapped);

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-2".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let remapped = bootstrap.snapshot();
            let layer = remapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-layer-2")
                .unwrap();
            assert_eq!(layer.output_id, Some(OutputId::from("out-1")));
            assert_eq!(
                layer.layer_metadata,
                Some(LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                })
            );
            assert!(layer.mapped);
            assert_topology_matches_known_surfaces(&remapped);
            assert_output_summary_matches_topology(&remapped);
            assert_seat_summary_matches_topology(&remapped);
        }

        #[test]
        fn bootstrap_removes_layer_surface_from_topology_when_smithay_layer_is_lost() {
            let mut bootstrap = test_bootstrap("wayland-test-layer-3");
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-3".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "overlay".into(),
                        tier: LayerSurfaceTier::Overlay,
                        keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                        exclusive_zone: LayerExclusiveZone::DontCare,
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();
            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_loss("wl-layer-3");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert!(snapshot.runtime.state.known_surfaces.layers.is_empty());
            assert!(snapshot
                .topology
                .surfaces
                .iter()
                .all(|surface| surface.id != "wl-layer-3"));
        }

        #[test]
        fn bootstrap_removes_topology_surface_when_smithay_surface_is_lost() {
            let mut bootstrap = test_bootstrap("wayland-test-6");

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-801".into(),
                    window_id: WindowId::from("smithay-window-801"),
                    output_id: None,
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();
            assert!(bootstrap
                .controller
                .app()
                .session()
                .topology()
                .surface("wl-surface-801")
                .is_some());

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_loss("wl-surface-801");

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.runtime.state.known_surfaces.toplevels.len(), 0);
            assert_eq!(snapshot.runtime.state.tracked_surface_count, 0);
            assert_eq!(snapshot.topology_surface_count, 0);
            assert!(snapshot.topology.surfaces.is_empty());
            assert!(bootstrap
                .controller
                .app()
                .session()
                .topology()
                .surface("wl-surface-801")
                .is_none());
        }

        #[test]
        fn bootstrap_unmaps_and_remaps_topology_surface_when_smithay_surface_buffer_changes() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let mut runtime = test_runtime("wayland-test-7");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-901".into(),
                    window_id: WindowId::from("smithay-window-901"),
                    output_id: None,
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-7".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);
            assert!(
                bootstrap
                    .controller
                    .app()
                    .session()
                    .topology()
                    .surface("wl-surface-901")
                    .unwrap()
                    .mapped
            );

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_unmap("wl-surface-901");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let unmapped = bootstrap.snapshot();
            let surface = unmapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-901")
                .unwrap();
            assert!(!surface.mapped);
            assert!(unmapped
                .topology
                .outputs
                .iter()
                .find(|output| output.snapshot.id == OutputId::from("out-1"))
                .unwrap()
                .mapped_surface_ids
                .is_empty());
            assert_eq!(unmapped.runtime.state.known_surfaces.toplevels.len(), 1);

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-901".into(),
                    window_id: WindowId::from("smithay-window-901"),
                    output_id: None,
                },
            );
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let remapped = bootstrap.snapshot();
            let surface = remapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-901")
                .unwrap();
            assert!(surface.mapped);
            assert_eq!(
                surface.window_id.as_ref(),
                Some(&WindowId::from("smithay-window-901"))
            );
            assert_topology_matches_known_surfaces(&remapped);
        }

        #[test]
        fn bootstrap_cascades_popup_unmap_and_removal_from_parent_surface() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let mut runtime = test_runtime("wayland-test-8");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-1001".into(),
                    window_id: WindowId::from("smithay-window-1001"),
                    output_id: None,
                },
            );
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-surface-1002".into(),
                    output_id: None,
                    parent_surface_id: "wl-surface-1001".into(),
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-8".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 2);

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_unmap("wl-surface-1001");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let unmapped = bootstrap.snapshot();
            assert!(
                !unmapped
                    .topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == "wl-surface-1001")
                    .unwrap()
                    .mapped
            );
            assert!(
                !unmapped
                    .topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == "wl-surface-1002")
                    .unwrap()
                    .mapped
            );

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_loss("wl-surface-1001");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let removed = bootstrap.snapshot();
            assert!(removed
                .topology
                .surfaces
                .iter()
                .all(|surface| surface.id != "wl-surface-1001"));
            assert!(removed
                .topology
                .surfaces
                .iter()
                .all(|surface| surface.id != "wl-surface-1002"));
        }

        #[test]
        fn bootstrap_preserves_output_for_popup_parented_to_layer_surface() {
            let mut bootstrap = test_bootstrap("wayland-test-9");

            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-51".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            bootstrap
                .runtime
                .state_mut()
                .track_test_popup_parent("wl-popup-51", "wl-layer-51");
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-51".into(),
                    output_id: Some(OutputId::from("out-1")),
                    parent_surface_id: "wl-layer-51".into(),
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let popup = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-popup-51")
                .unwrap();
            assert_eq!(popup.role, SurfaceRole::Popup);
            assert_eq!(popup.parent_surface_id.as_deref(), Some("wl-layer-51"));
            assert_eq!(popup.output_id, Some(OutputId::from("out-1")));
        }

        #[test]
        fn bootstrap_snapshot_preserves_xdg_popup_configure_metadata() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let mut runtime = test_runtime("wayland-test-popup-meta-1");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-meta-1".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-meta-1".into(),
                },
            );
            runtime.state_mut().set_test_popup_configure_snapshot(
                "wl-popup-meta-1",
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: Some(18),
                    pending_configure_count: 0,
                    last_reposition_token: Some(77),
                    reactive: true,
                    geometry: (15, 25, 320, 180),
                    last_grab_serial: Some(14),
                    grab_requested: true,
                    last_request_kind: Some("grab".into()),
                    request_count: 2,
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-popup-meta-1".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.runtime.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: Some(18),
                    pending_configure_count: 0,
                    last_reposition_token: Some(77),
                    reactive: true,
                    geometry: (15, 25, 320, 180),
                    last_grab_serial: Some(14),
                    grab_requested: true,
                    last_request_kind: Some("grab".into()),
                    request_count: 2,
                }
            );
            assert_topology_matches_known_surfaces(&snapshot);
        }

        #[test]
        fn bootstrap_snapshot_preserves_xdg_toplevel_size_constraints() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let mut runtime = test_runtime("wayland-test-xdg-size-1");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-size-1".into(),
                    window_id: WindowId::from("smithay-window-size-1"),
                    output_id: None,
                },
            );
            runtime.state_mut().set_test_toplevel_metadata_snapshot(
                "wl-surface-size-1",
                crate::smithay_state::SmithayXdgToplevelMetadataSnapshot {
                    title: Some("settings".into()),
                    app_id: Some("spiders.settings".into()),
                    parent_surface_id: None,
                    min_size: Some((800, 600)),
                    max_size: Some((2560, 1440)),
                    window_geometry: Some((30, 40, 1440, 900)),
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-xdg-size-1".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].metadata,
                crate::smithay_state::SmithayXdgToplevelMetadataSnapshot {
                    title: Some("settings".into()),
                    app_id: Some("spiders.settings".into()),
                    parent_surface_id: None,
                    min_size: Some((800, 600)),
                    max_size: Some((2560, 1440)),
                    window_geometry: Some((30, 40, 1440, 900)),
                }
            );
            assert_topology_matches_known_surfaces(&snapshot);
        }

        #[test]
        fn runtime_snapshot_exposes_clipboard_selection_inspection() {
            let mut runtime = test_runtime("wayland-test-clipboard-1");
            runtime.state_mut().set_test_clipboard_selection(Some(
                crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "data-device".into(),
                },
            ));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.clipboard_selection.target, "clipboard");
            assert_eq!(
                snapshot.state.clipboard_selection.selection,
                Some(crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "data-device".into(),
                })
            );
            assert!(snapshot
                .state
                .clipboard_selection
                .focused_client_id
                .is_none());
        }

        #[test]
        fn runtime_snapshot_exposes_clipboard_focus_inspection() {
            let mut runtime = test_runtime("wayland-test-clipboard-2");
            runtime
                .state_mut()
                .set_test_clipboard_focus_client_id(Some("client-9"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot
                    .state
                    .clipboard_selection
                    .focused_client_id
                    .as_deref(),
                Some("client-9")
            );
        }

        #[test]
        fn runtime_snapshot_exposes_primary_selection_inspection() {
            let mut runtime = test_runtime("wayland-test-primary-1");
            runtime.state_mut().set_test_primary_selection(Some(
                crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "primary-selection".into(),
                },
            ));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.primary_selection.target, "primary");
            assert_eq!(
                snapshot.state.primary_selection.selection,
                Some(crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "primary-selection".into(),
                })
            );
            assert!(snapshot.state.primary_selection.focused_client_id.is_none());
        }

        #[test]
        fn runtime_snapshot_exposes_primary_focus_inspection() {
            let mut runtime = test_runtime("wayland-test-primary-2");
            runtime
                .state_mut()
                .set_test_primary_focus_client_id(Some("client-13"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot
                    .state
                    .primary_selection
                    .focused_client_id
                    .as_deref(),
                Some("client-13")
            );
        }

        #[test]
        fn runtime_snapshot_exposes_selection_protocol_support() {
            let runtime = test_runtime("wayland-test-selection-support");

            let snapshot = runtime.snapshot();
            assert!(snapshot.state.selection_protocols.data_device);
            assert!(snapshot.state.selection_protocols.primary_selection);
            assert!(snapshot.state.selection_protocols.wlr_data_control);
            assert!(snapshot.state.selection_protocols.ext_data_control);
        }

        #[test]
        fn runtime_snapshot_exposes_seat_focus_inspection() {
            let mut runtime = test_runtime("wayland-test-seat-focus");
            runtime
                .state_mut()
                .set_test_focused_surface_id(Some("wl-surface-501"));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.seat.name, "smithay-test-seat");
            assert_eq!(
                snapshot.state.seat.focused_surface_id.as_deref(),
                Some("wl-surface-501")
            );
        }

        #[test]
        fn runtime_snapshot_exposes_focused_role_and_window_summary() {
            let mut runtime = test_runtime("wayland-test-seat-focus-summary");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-601".into(),
                    window_id: WindowId::from("smithay-window-601"),
                    output_id: None,
                },
            );
            runtime
                .state_mut()
                .set_test_focused_surface_id(Some("wl-surface-601"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.seat.focused_surface_role.as_deref(),
                Some("toplevel")
            );
            assert_eq!(
                snapshot.state.seat.focused_window_id,
                Some(WindowId::from("smithay-window-601"))
            );
            assert!(snapshot.state.seat.focused_output_id.is_none());
        }

        #[test]
        fn runtime_snapshot_exposes_focused_output_summary() {
            let mut runtime = test_runtime("wayland-test-seat-focus-output");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-focus-runtime-1".into(),
                    output_id: OutputId::from("out-5"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(8),
                    },
                },
            );
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-focus-runtime-1".into(),
                    output_id: Some(OutputId::from("out-5")),
                    parent_surface_id: "wl-layer-focus-runtime-1".into(),
                },
            );
            runtime
                .state_mut()
                .track_test_popup_parent("wl-popup-focus-runtime-1", "wl-layer-focus-runtime-1");
            runtime
                .state_mut()
                .set_test_focused_surface_id(Some("wl-popup-focus-runtime-1"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.seat.focused_surface_role.as_deref(),
                Some("popup")
            );
            assert_eq!(
                snapshot.state.seat.focused_output_id,
                Some(OutputId::from("out-5"))
            );
        }

        #[test]
        fn runtime_snapshot_exposes_cursor_inspection() {
            let mut runtime = test_runtime("wayland-test-seat-cursor");
            runtime
                .state_mut()
                .set_test_cursor_image("named:Crosshair", None);

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.seat.cursor_image, "named:Crosshair");
            assert!(snapshot.state.seat.cursor_surface_id.is_none());
        }

        #[test]
        fn runtime_snapshot_exposes_output_inspection() {
            let mut runtime = test_runtime("wayland-test-output-state");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.outputs.known_output_ids,
                vec![OutputId::from("out-1"), OutputId::from("out-2")]
            );
            assert_eq!(
                snapshot.state.outputs.active_output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.state.outputs.active_output_attached_surface_count,
                0
            );
        }

        #[test]
        fn runtime_snapshot_exposes_output_attachment_summary() {
            let mut runtime = test_runtime("wayland-test-output-summary");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-summary-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(12),
                    },
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.outputs.active_output_attached_surface_count,
                1
            );
            assert_eq!(snapshot.state.outputs.mapped_surface_count, 1);
        }

        #[test]
        fn bootstrap_apply_pending_discovery_events_returns_zero_when_empty() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime("wayland-test-4");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-4".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            let applied = bootstrap.apply_pending_discovery_events().unwrap();

            assert_eq!(applied, 0);
            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.runtime.state.pending_discovery_event_count, 0);
            assert_eq!(snapshot.topology_surface_count, 0);
            assert_eq!(bootstrap.controller.phase(), ControllerPhase::Pending);
        }

        #[test]
        fn bootstrap_applies_pending_seat_focus_discovery_events_to_controller() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime("wayland-test-seat-focus-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-seat-focus-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-seat-focus-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(8),
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .runtime
                .state_mut()
                .record_test_seat_focus_event(Some("wl-seat-focus-1"));

            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            let seat = snapshot
                .topology
                .seats
                .iter()
                .find(|seat| seat.name == "smithay-test-seat")
                .unwrap();
            assert_eq!(seat.focused_window_id, None);
            assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));
        }

        #[test]
        fn bootstrap_applies_pending_output_activation_discovery_events_to_controller() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state.outputs.push(spiders_shared::wm::OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: spiders_shared::wm::OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime("wayland-test-output-activate-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-output-activate-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .runtime
                .state_mut()
                .activate_output_id(OutputId::from("out-2"));

            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-2"))
            );
        }

        #[test]
        fn bootstrap_applies_pending_output_lost_discovery_events_to_controller() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime("wayland-test-output-lost-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-output-lost-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-output-lost-layer-1".into(),
                    output_id: OutputId::from("out-2"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(10),
                    },
                },
            );
            bootstrap
                .runtime
                .state_mut()
                .activate_output_id(OutputId::from("out-2"));
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 2);

            bootstrap
                .runtime
                .state_mut()
                .remove_output_id(&OutputId::from("out-2"));

            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            assert!(snapshot
                .topology
                .outputs
                .iter()
                .all(|output| output.snapshot.id != OutputId::from("out-2")));
            assert_eq!(snapshot.topology.active_output_id, None);
            let layer = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-output-lost-layer-1")
                .unwrap();
            assert_eq!(layer.output_id, None);
        }

        #[test]
        fn bootstrap_applies_pending_workspace_activate_action_to_controller_and_export() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state
                .workspaces
                .push(spiders_shared::wm::WorkspaceSnapshot {
                    id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["2".into()],
                    focused: false,
                    visible: false,
                    effective_layout: Some(spiders_shared::wm::LayoutRef {
                        name: "master-stack".into(),
                    }),
                });
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime("wayland-test-workspace-activate-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-workspace-activate-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            initialize_smithay_workspace_export(
                &bootstrap.controller,
                bootstrap.runtime.state_mut(),
            );
            bootstrap.runtime.state_mut().queue_workspace_action(
                spiders_shared::api::WmAction::ActivateWorkspace {
                    workspace_id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                },
            );

            let applied = bootstrap.apply_pending_workspace_actions().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            assert_eq!(
                bootstrap.controller.state_snapshot().current_workspace_id,
                Some(spiders_shared::ids::WorkspaceId::from("ws-2"))
            );
            assert!(
                snapshot
                    .runtime
                    .state
                    .outputs
                    .known_output_ids
                    .contains(&OutputId::from("out-1"))
                    || snapshot.runtime.state.outputs.active_output_id.is_none()
            );
        }

        #[test]
        fn bootstrap_applies_pending_workspace_assign_action_to_controller_and_export() {
            let runtime_service = test_runtime_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            state
                .workspaces
                .push(spiders_shared::wm::WorkspaceSnapshot {
                    id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["2".into()],
                    focused: false,
                    visible: false,
                    effective_layout: Some(spiders_shared::wm::LayoutRef {
                        name: "master-stack".into(),
                    }),
                });
            let controller =
                crate::CompositorController::initialize(runtime_service, config, state).unwrap();
            let runtime = test_runtime("wayland-test-workspace-assign-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-workspace-assign-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
            };

            initialize_smithay_workspace_export(
                &bootstrap.controller,
                bootstrap.runtime.state_mut(),
            );
            bootstrap.runtime.state_mut().queue_workspace_action(
                spiders_shared::api::WmAction::AssignWorkspace {
                    workspace_id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                    output_id: OutputId::from("out-2"),
                },
            );

            let applied = bootstrap.apply_pending_workspace_actions().unwrap();

            assert_eq!(applied, 1);
            assert_eq!(
                bootstrap
                    .controller
                    .state_snapshot()
                    .workspace_by_id(&spiders_shared::ids::WorkspaceId::from("ws-2"))
                    .unwrap()
                    .output_id,
                Some(OutputId::from("out-2"))
            );
        }
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{
    bootstrap_winit, bootstrap_winit_controller, initialize_smithay_workspace_export,
    initialize_winit_controller, SmithayBootstrap, SmithayBootstrapSnapshot,
    SmithayBootstrapTopologySnapshot, SmithayRuntimeError, SmithayRuntimeSnapshot,
    SmithayStartupReport, SmithayWinitRuntime,
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

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmithayRuntimeSnapshot;

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmithayBootstrapSnapshot;
