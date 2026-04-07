#[cfg(feature = "tty-preview")]
use drm_fourcc::DrmFourcc;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::compositor::DrmCompositor;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::DrmEvent;
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::Device as ControlDevice;
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::connector::State as ConnectorState;
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::crtc;
#[cfg(feature = "tty-preview")]
use smithay::reexports::drm::control::ModeTypeFlags;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::DrmNode;
#[cfg(feature = "libseat")]
use smithay::backend::session::Event as SessionEvent;
#[cfg(feature = "libseat")]
use smithay::backend::session::Session;
#[cfg(feature = "libseat")]
use smithay::backend::session::libseat::LibSeatSession;
#[cfg(feature = "tty-preview")]
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::gles::GlesRenderer;
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::multigpu::{GpuManager, gbm::GbmGlesBackend};
#[cfg(feature = "tty-preview")]
use smithay::backend::renderer::ImportDma;
#[cfg(feature = "tty-preview")]
use smithay::backend::udev::{UdevBackend, UdevEvent, all_gpus, primary_gpu};
#[cfg(feature = "tty-preview")]
use smithay::output::OutputModeSource;
use smithay::reexports::calloop::EventLoop;
#[cfg(feature = "tty-preview")]
use smithay::utils::DeviceFd;
use tracing::info;

#[cfg(feature = "tty-preview")]
use crate::backend::drm::{DrmDeviceRecord, wrap_drm_device};
use crate::backend::drm::TtyDrmBackendState;
use crate::backend::output::TtyOutputState;
#[cfg(feature = "tty-preview")]
use crate::backend::output::{drm_physical_properties, register_existing_output, remove_output_from_runtime};
use crate::backend::session::{BackendSession, TtySessionHandle, TtySessionState};
use crate::backend::{BackendState, TtyBackendState};
use crate::state::SpidersWm;

#[cfg(feature = "tty-preview")]
type TtyGpuManager = GpuManager<GbmGlesBackend<GlesRenderer, smithay::backend::drm::DrmDeviceFd>>;

pub(crate) fn init_tty(
    #[cfg(feature = "libseat")] event_loop: &mut EventLoop<'static, SpidersWm>,
    #[cfg(not(feature = "libseat"))] _event_loop: &mut EventLoop<'static, SpidersWm>,
    state: &mut SpidersWm,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "libseat")]
    if let Some(session) = try_init_libseat_session(event_loop, state)? {
        #[cfg(feature = "tty-preview")]
        init_tty_preview_sources(event_loop, &session)?;

        let seat_name = session.seat_name.clone();
        let active = session.active;
        let backend = TtyBackendState {
            session: BackendSession::Tty(session),
            outputs: Vec::<TtyOutputState>::new(),
            drm: TtyDrmBackendState::default(),
        };
        state.backend = Some(BackendState::Tty(backend));
        #[cfg(feature = "tty-preview")]
        init_tty_preview_drm_state(event_loop, state)?;
        info!(seat = %seat_name, active, "tty backend initialized libseat session");
        return Err("tty backend session is initialized, but drm/kms startup is not implemented yet".into());
    }

    let backend = TtyBackendState {
        session: BackendSession::Tty(TtySessionState {
            seat_name: "seat0".into(),
            active: false,
            handle: TtySessionHandle::Placeholder,
        }),
        outputs: Vec::<TtyOutputState>::new(),
        drm: TtyDrmBackendState::default(),
    };
    state.backend = Some(BackendState::Tty(backend));
    #[cfg(not(feature = "libseat"))]
    info!(seat = "seat0", "tty backend selected without libseat support");
    #[cfg(feature = "libseat")]
    info!(seat = "seat0", "tty backend selected but libseat session could not be started");
    Err("tty backend is not implemented yet; enable the 'libseat' feature for real session startup work".into())
}

#[cfg(feature = "tty-preview")]
fn init_tty_preview_sources(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    session: &TtySessionState,
) -> Result<(), Box<dyn std::error::Error>> {
    log_tty_preview_gpu_candidates(session)?;

    if let TtySessionHandle::LibSeat(libseat_session) = &session.handle {
        let mut input = input::Libinput::new_with_udev::<LibinputSessionInterface<LibSeatSession>>(
            libseat_session.clone().into(),
        );
        input
            .udev_assign_seat(&session.seat_name)
            .map_err(|_| std::io::Error::other("failed to assign libinput to seat"))?;
        let backend = LibinputInputBackend::new(input);
        event_loop.handle().insert_source(backend, |event, _, _state| {
            info!(event = ?event, "tty preview libinput event");
        })?;
    }

    let udev = UdevBackend::new(&session.seat_name)?;
    event_loop.handle().insert_source(udev, |event, _, state| match event {
        UdevEvent::Added { device_id, path } => {
            info!(?device_id, path = %path.display(), "tty preview udev device added");
            state.schedule_tty_redraw();
        }
        UdevEvent::Changed { device_id } => {
            info!(?device_id, "tty preview udev device changed");
            state.handle_tty_drm_changed();
        }
        UdevEvent::Removed { device_id } => {
            info!(?device_id, "tty preview udev device removed");
            state.handle_tty_drm_changed();
        }
    })?;

    Ok(())
}

#[cfg(feature = "tty-preview")]
fn log_tty_preview_gpu_candidates(session: &TtySessionState) -> Result<(), Box<dyn std::error::Error>> {
    let seat_name = &session.seat_name;
    let primary = primary_gpu(seat_name)?;
    let gpus = all_gpus(seat_name)?;
    info!(seat = seat_name, primary_gpu = ?primary, gpu_count = gpus.len(), "tty preview enumerated gpus");
    for gpu in gpus {
        info!(seat = seat_name, path = %gpu.display(), "tty preview gpu candidate");
    }

    Ok(())
}

#[cfg(feature = "tty-preview")]
fn init_tty_preview_drm_state(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    state: &mut SpidersWm,
) -> Result<(), Box<dyn std::error::Error>> {
    let (seat_name, devices) = {
        let Some(BackendState::Tty(backend)) = state.backend.as_mut() else {
            return Ok(());
        };
        let BackendSession::Tty(session) = &backend.session else {
            return Ok(());
        };

        let seat_name = session.seat_name.clone();
        let devices = create_drm_records(session)?;
        if devices.is_empty() {
            return Ok(());
        }
        (seat_name, devices)
    };

    let mut all_outputs = Vec::new();
    let mut device_records = Vec::new();
    for (device_record, notifier) in devices {
        register_drm_notifier(event_loop, device_record.node, notifier)?;
        let mut outputs = collect_connector_outputs(&seat_name, &device_record)?;
        all_outputs.append(&mut outputs);
        device_records.push(device_record);
    }

    for (index, output_record) in all_outputs.iter().enumerate() {
        register_existing_output(
            state,
            output_record.output_id.clone(),
            output_record.connector_name.clone(),
            &output_record.output,
            output_record.mode,
            smithay::utils::Transform::Normal,
            ((index as i32) * output_record.mode.size.w, 0).into(),
        );
    }

    if let Some(BackendState::Tty(backend)) = state.backend.as_mut() {
        backend.outputs = all_outputs;
        backend.drm.devices = device_records;
        initialize_drm_surfaces(&seat_name, backend)?;
        initialize_tty_renderer_state(state)?;
        state.try_tty_initial_scanout();
    }

    info!(seat = %seat_name, "tty preview persisted drm device state");
    Ok(())
}

#[cfg(feature = "tty-preview")]
fn create_drm_records(
    session: &TtySessionState,
) -> Result<Vec<(DrmDeviceRecord, smithay::backend::drm::DrmDeviceNotifier)>, Box<dyn std::error::Error>> {
    let seat_name = &session.seat_name;
    let primary = primary_gpu(seat_name)?;
    let all = all_gpus(seat_name)?;
    let mut ordered = Vec::new();
    if let Some(primary) = primary {
        ordered.push(primary.clone());
    }
    for gpu in all {
        if ordered.iter().any(|existing| existing == &gpu) {
            continue;
        }
        ordered.push(gpu);
    }

    let mut records = Vec::new();
    if let TtySessionHandle::LibSeat(libseat_session) = &session.handle {
        for path in ordered {
            let fd = match libseat_session
                .clone()
                .open(path.as_path(), rustix::fs::OFlags::RDWR | rustix::fs::OFlags::CLOEXEC)
            {
                Ok(fd) => fd,
                Err(error) => {
                    info!(path = %path.display(), %error, "tty preview failed to open drm gpu");
                    continue;
                }
            };
            let node = match DrmNode::from_path(path.as_path()) {
                Ok(node) => node,
                Err(error) => {
                    info!(path = %path.display(), %error, "tty preview failed to parse drm node");
                    continue;
                }
            };
            let record = wrap_drm_device(node, DeviceFd::from(fd))
                .map_err(|error| std::io::Error::other(format!("failed to create drm device: {error}")))?;
            records.push(record);
        }
    }

    Ok(records)
}

#[cfg(feature = "tty-preview")]
fn collect_connector_outputs(
    seat_name: &str,
    record: &DrmDeviceRecord,
) -> Result<Vec<TtyOutputState>, Box<dyn std::error::Error>> {
    let resources = record
        .device
        .resource_handles()
        .map_err(|error| std::io::Error::other(format!("failed to read drm resources: {error}")))?;
    info!(
        seat = seat_name,
        node = ?record.node,
        connectors = resources.connectors().len(),
        crtcs = resources.crtcs().len(),
        encoders = resources.encoders().len(),
        atomic = record.device.is_atomic(),
        render_node = ?record.render.render_node,
        "tty preview opened primary drm device"
    );

    let mut outputs = Vec::new();
    for connector in resources.connectors() {
        let info = record
            .device
            .get_connector(*connector, true)
            .map_err(|error| std::io::Error::other(format!("failed to query connector: {error}")))?;
        if info.state() != ConnectorState::Connected {
            continue;
        }
        let connector_name = format!("{:?}-{}", info.interface(), info.interface_id());
        let Some((mode, drm_mode)) = select_connector_mode(&info) else {
            info!(seat = seat_name, connector_name, "tty preview skipping connector without mode");
            continue;
        };
        let (phys_w, phys_h) = info.size().unwrap_or((0, 0));
        let physical = drm_physical_properties("DRM".into(), connector_name.clone());
        let output = smithay::output::Output::new(
            connector_name.clone(),
            smithay::output::PhysicalProperties {
                size: (phys_w as i32, phys_h as i32).into(),
                subpixel: info.subpixel().into(),
                ..physical.clone()
            },
        );
        info!(
            seat = seat_name,
            node = ?record.node,
            connector = ?connector,
            connector_name,
            state = ?info.state(),
            mode_width = mode.size.w,
            mode_height = mode.size.h,
            refresh = mode.refresh,
            "tty preview drm connector"
        );
        outputs.push(TtyOutputState {
            output,
            output_id: format!("drm:{}:{}", seat_name, connector_name),
            connector_name,
            drm_node: Some(record.node),
            connector: *connector,
            mode,
            drm_mode,
            physical,
        });
    }
    Ok(outputs)
}

#[cfg(feature = "tty-preview")]
fn select_connector_mode(
    connector: &smithay::reexports::drm::control::connector::Info,
) -> Option<(smithay::output::Mode, smithay::reexports::drm::control::Mode)> {
    let drm_mode = connector
        .modes()
        .iter()
        .copied()
        .find(|mode| mode.mode_type().contains(ModeTypeFlags::PREFERRED))
        .or_else(|| connector.modes().first().copied())?;

    Some((
        smithay::output::Mode {
            size: (drm_mode.size().0 as i32, drm_mode.size().1 as i32).into(),
            refresh: (drm_mode.vrefresh() as i32) * 1000,
        },
        drm_mode,
    ))
}

#[cfg(feature = "tty-preview")]
fn register_drm_notifier(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    node: DrmNode,
    notifier: smithay::backend::drm::DrmDeviceNotifier,
) -> Result<(), Box<dyn std::error::Error>> {
    event_loop.handle().insert_source(notifier, move |event, metadata, state| {
        state.handle_tty_drm_event(node, event, *metadata);
    })?;
    Ok(())
}

#[cfg(feature = "tty-preview")]
impl SpidersWm {
    fn handle_tty_drm_event(
        &mut self,
        node: DrmNode,
        event: DrmEvent,
        metadata: Option<smithay::backend::drm::DrmEventMetadata>,
    ) {
        match event {
            DrmEvent::VBlank(crtc) => {
                info!(node = ?node, ?crtc, ?metadata, "tty preview drm vblank");
                self.tty_frame_submitted(node, crtc);
                self.schedule_tty_redraw();
            }
            DrmEvent::Error(error) => {
                info!(node = ?node, ?error, "tty preview drm device error");
            }
        }
    }

    fn handle_tty_session_activated(&mut self, seat_name: String) {
        info!(seat = %seat_name, "tty preview session activated; refreshing drm outputs");
        self.handle_tty_drm_changed();
        self.schedule_tty_redraw();
    }

    fn handle_tty_session_paused(&mut self, seat_name: String) {
        info!(seat = %seat_name, "tty preview session paused");
        self.reset_tty_scanout_state();
    }

    fn handle_tty_drm_changed(&mut self) {
        let (stale_outputs, updates, seat_name) = {
            let Some(BackendState::Tty(backend)) = self.backend.as_mut() else {
                return;
            };
            let seat_name = match &backend.session {
                BackendSession::Tty(session) => session.seat_name.clone(),
                _ => return,
            };
            if backend.drm.devices.is_empty() {
                return;
            }

            let previous_outputs = std::mem::take(&mut backend.outputs);
            for device in &mut backend.drm.devices {
                device.surfaces.clear();
            }

            let mut next_outputs = Vec::new();
            for device in &backend.drm.devices {
                let Ok(mut outputs) = collect_connector_outputs(&seat_name, device) else {
                    continue;
                };
                next_outputs.append(&mut outputs);
            }

            let stale = previous_outputs
                .iter()
                .filter(|previous| {
                    !next_outputs.iter().any(|next| next.output_id == previous.output_id)
                })
                .map(|output| (output.output.clone(), output.output_id.clone()))
                .collect::<Vec<_>>();

            let updates = next_outputs
                .iter()
                .enumerate()
                .filter_map(|(index, output)| {
                    let changed = previous_outputs
                        .iter()
                        .find(|previous| previous.output_id == output.output_id)
                        .map(|previous| !previous.same_render_state(output))
                        .unwrap_or(true);
                    changed.then_some((
                        output.output_id.clone(),
                        output.connector_name.clone(),
                        output.output.clone(),
                        output.mode,
                        ((index as i32) * output.mode.size.w, 0).into(),
                    ))
                })
                .collect::<Vec<_>>();

            backend.outputs = next_outputs;
            (stale, updates, seat_name)
        };

        for (output, output_id) in stale_outputs {
            self.space.unmap_output(&output);
            remove_output_from_runtime(self, output_id);
        }

        for (output_id, output_name, output, mode, location) in updates {
            register_existing_output(
                self,
                output_id,
                output_name,
                &output,
                mode,
                smithay::utils::Transform::Normal,
                location,
            );
        }

        if let Some(BackendState::Tty(backend)) = self.backend.as_mut() {
            let _ = initialize_drm_surfaces(&seat_name, backend);
        }
        let _ = initialize_tty_renderer_state(self);
        self.schedule_tty_redraw();
    }

    fn reset_tty_scanout_state(&mut self) {
        let Some(BackendState::Tty(backend)) = self.backend.as_mut() else {
            return;
        };
        for device in &mut backend.drm.devices {
            for surface in &mut device.surfaces {
                surface.compositor = None;
                let _ = surface.surface.take();
            }
            device.surfaces.clear();
        }
    }

    fn tty_frame_submitted(&mut self, node: DrmNode, crtc: crtc::Handle) {
        let Some(BackendState::Tty(backend)) = self.backend.as_mut() else {
            return;
        };
        let Some(device) = backend.drm.devices.iter_mut().find(|device| device.node == node) else {
            return;
        };
        let Some(surface) = device.surfaces.iter_mut().find(|surface| surface.crtc == crtc) else {
            return;
        };
        let Some(compositor) = surface.compositor.as_mut() else {
            return;
        };

        if let Err(error) = compositor.frame_submitted() {
            info!(node = ?node, ?crtc, ?error, "tty preview frame_submitted failed");
        }
    }

    fn try_tty_initial_scanout(&mut self) {
        self.try_tty_blank_frame();
    }

    fn schedule_tty_redraw(&self) {
        let handle = self.event_loop.clone();
        handle.insert_idle(|state| {
            state.try_tty_blank_frame();
        });
    }

    fn try_tty_blank_frame(&mut self) {
        let output = match self.backend.as_ref() {
            Some(BackendState::Tty(backend)) => {
                let active = matches!(&backend.session, BackendSession::Tty(session) if session.active);
                active.then(|| backend.outputs.first().map(|output| output.output.clone())).flatten()
            }
            _ => None,
        };
        let Some(output) = output else {
            return;
        };

        let elements = self.tty_collect_space_elements(&output);

        let mut queued_frame = false;

        let Some(BackendState::Tty(backend)) = self.backend.as_mut() else {
            return;
        };
        let Some(primary_device) = backend.drm.devices.first_mut() else {
            return;
        };
        let Some(render_node) = primary_device.render.render_node else {
            return;
        };
        let Some(gpu_manager) = primary_device.render.gpu_manager.as_mut() else {
            return;
        };

        let Ok(mut renderer) = gpu_manager.single_renderer(&render_node) else {
            return;
        };

        for surface in &mut primary_device.surfaces {
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
                    match compositor.queue_frame(()) {
                        Ok(()) => {
                            queued_frame = true;
                            info!(
                                node = ?primary_device.node,
                                connector = ?surface.connector,
                                crtc = ?surface.crtc,
                                "tty preview queued scene frame"
                            );
                            break;
                        }
                        Err(error) => {
                            info!(
                                node = ?primary_device.node,
                                connector = ?surface.connector,
                                crtc = ?surface.crtc,
                                ?error,
                                "tty preview failed to queue scene frame"
                            );
                        }
                    }
                }
                Err(error) => {
                    info!(
                        node = ?primary_device.node,
                        connector = ?surface.connector,
                        crtc = ?surface.crtc,
                        ?error,
                        "tty preview failed to render scene frame"
                    );
                }
            }
        }

        if queued_frame {
            self.send_frames_for_windows(&output);
        }
    }

    fn tty_collect_space_elements(
        &mut self,
        output: &smithay::output::Output,
    ) -> Vec<smithay::desktop::space::SpaceRenderElements<
        GlesRenderer,
        smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement<GlesRenderer>,
    >> {
        let mut backend = match self.backend.take() {
            Some(BackendState::Tty(backend)) => backend,
            Some(other) => {
                self.backend = Some(other);
                return Vec::new();
            }
            None => return Vec::new(),
        };

        let Some(primary_device) = backend.drm.devices.first_mut() else {
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

        self.notify_blocker_cleared();
        let elements = smithay::desktop::space::space_render_elements::<_, smithay::desktop::Window, _>(
            renderer.as_mut(),
            [&self.space],
            output,
            1.0,
        )
        .unwrap_or_default();
        self.backend = Some(BackendState::Tty(backend));
        elements
    }
}

#[cfg(feature = "tty-preview")]
fn initialize_drm_surfaces(
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
                info!(
                    seat = seat_name,
                    node = ?device.node,
                    connector = ?output.connector,
                    connector_name = output.connector_name,
                    "tty preview found no compatible crtc for connector"
                );
                continue;
            };

            match device.device.create_surface(crtc, output.drm_mode, &[output.connector]) {
                Ok(surface) => {
                    info!(
                        seat = seat_name,
                        node = ?device.node,
                        connector = ?output.connector,
                        connector_name = output.connector_name,
                        ?crtc,
                        "tty preview created drm surface candidate"
                    );
                    device.surfaces.push(crate::backend::drm::DrmSurfaceRecord {
                        connector: output.connector,
                        crtc,
                        surface: Some(surface),
                        compositor: None,
                    });
                }
                Err(error) => {
                    info!(
                        seat = seat_name,
                        node = ?device.node,
                        connector = ?output.connector,
                        connector_name = output.connector_name,
                        ?crtc,
                        %error,
                        "tty preview failed to create drm surface candidate"
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "tty-preview")]
fn initialize_tty_renderer_state(state: &mut SpidersWm) -> Result<(), Box<dyn std::error::Error>> {
    let Some(BackendState::Tty(backend)) = state.backend.as_mut() else {
        return Ok(());
    };
    let Some(primary_device) = backend.drm.devices.first_mut() else {
        return Ok(());
    };
    let Some(render_node) = primary_device.render.render_node else {
        return Ok(());
    };

    if primary_device.render.gpu_manager.is_none() {
        let mut api = GbmGlesBackend::default();
        api.add_node(render_node, primary_device.render.gbm.clone())?;
        primary_device.render.gpu_manager = Some(TtyGpuManager::new(api)?);
        info!(node = ?primary_device.node, ?render_node, "tty preview initialized gpu manager");
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
        state.dmabuf_global = Some(
            state
                .dmabuf_state
                .create_global_with_default_feedback::<SpidersWm>(&state.display_handle, &default_feedback),
        );
    }

    let renderer_formats = renderer
        .as_mut()
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect::<Vec<_>>();

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
                info!(
                    node = ?primary_device.node,
                    connector = ?surface.connector,
                    crtc = ?surface.crtc,
                    "tty preview created drm compositor candidate"
                );
                surface.compositor = Some(compositor);
                break;
            }
            Err(error) => {
                info!(
                    node = ?primary_device.node,
                    connector = ?surface.connector,
                    crtc = ?surface.crtc,
                    ?error,
                    "tty preview failed to create drm compositor candidate"
                );
            }
        }
    }

    info!(
        node = ?primary_device.node,
        render_node = ?primary_device.render.render_node,
        compositor_candidates = primary_device.surfaces.iter().filter(|s| s.compositor.is_some()).count(),
        "tty preview renderer state ready"
    );

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
    let connector_info = device
        .get_connector(connector, true)
        .map_err(|error| std::io::Error::other(format!("failed to read connector info: {error}")))?;

    if let Some(encoder) = connector_info.current_encoder().or_else(|| connector_info.encoders().first().copied()) {
        let encoder_info = device
            .get_encoder(encoder)
            .map_err(|error| std::io::Error::other(format!("failed to read encoder info: {error}")))?;
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

#[cfg(feature = "libseat")]
fn try_init_libseat_session(
    event_loop: &mut EventLoop<'static, SpidersWm>,
    state: &mut SpidersWm,
) -> Result<Option<TtySessionState>, Box<dyn std::error::Error>> {
    let (session, notifier) = match LibSeatSession::new() {
        Ok((session, notifier)) => (session, notifier),
        Err(error) => {
            info!(%error, "tty backend failed to open libseat session");
            return Ok(None);
        }
    };

    let seat_name = session.seat();
    let active = session.is_active();
    let notifier_seat = seat_name.clone();
    event_loop.handle().insert_source(notifier, move |event, _, state| {
        let active = matches!(event, SessionEvent::ActivateSession);
        if let Some(BackendState::Tty(backend)) = state.backend.as_mut()
            && let BackendSession::Tty(session) = &mut backend.session
        {
            session.active = active;
        }

        match event {
            SessionEvent::ActivateSession => {
                state.handle_tty_session_activated(notifier_seat.clone());
                info!(seat = %notifier_seat, "tty libseat session activated");
            }
            SessionEvent::PauseSession => {
                state.handle_tty_session_paused(notifier_seat.clone());
                info!(seat = %notifier_seat, "tty libseat session paused");
            }
        }
    })?;

    let _ = state;

    Ok(Some(TtySessionState {
        seat_name,
        active,
        handle: TtySessionHandle::LibSeat(session),
    }))
}
