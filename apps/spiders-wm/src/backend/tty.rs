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
#[cfg(feature = "libseat")]
use smithay::backend::session::Event as SessionEvent;
#[cfg(feature = "libseat")]
use smithay::backend::session::Session;
#[cfg(feature = "libseat")]
use smithay::backend::session::libseat::LibSeatSession;
#[cfg(feature = "tty-preview")]
use smithay::backend::drm::DrmNode;
#[cfg(feature = "tty-preview")]
use smithay::backend::udev::{all_gpus, primary_gpu};
use smithay::reexports::calloop::EventLoop;
#[cfg(feature = "tty-preview")]
use smithay::utils::DeviceFd;
use tracing::info;

#[cfg(feature = "tty-preview")]
use crate::backend::drm::{DrmDeviceRecord, wrap_drm_device};
use crate::backend::drm::TtyDrmBackendState;
use crate::backend::output::TtyOutputState;
#[cfg(feature = "tty-preview")]
use crate::backend::output::{
    assign_tty_output_locations, drm_physical_properties, register_existing_output,
    remove_output_from_runtime,
};
#[cfg(feature = "tty-preview")]
use crate::backend::tty_drm::{
    initialize_drm_surfaces, initialize_tty_device_surfaces, initialize_tty_renderer_state,
    initialize_tty_renderer_state_for_node, reset_tty_device_scanout_state,
};
#[cfg(feature = "tty-preview")]
use crate::backend::tty_setup::init_tty_preview_sources;
use crate::backend::session::{BackendSession, TtySessionHandle, TtySessionState};
use crate::backend::{BackendState, TtyBackendState};
use crate::state::SpidersWm;

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
            redraw_pending: false,
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
        redraw_pending: false,
    };
    state.backend = Some(BackendState::Tty(backend));
    #[cfg(not(feature = "libseat"))]
    info!(seat = "seat0", "tty backend selected without libseat support");
    #[cfg(feature = "libseat")]
    info!(seat = "seat0", "tty backend selected but libseat session could not be started");
    Err("tty backend is not implemented yet; enable the 'libseat' feature for real session startup work".into())
}

#[cfg(not(feature = "tty-preview"))]
impl SpidersWm {
    pub(crate) fn schedule_tty_redraw(&mut self) {}
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

    assign_tty_output_locations(&mut all_outputs, &[]);

    for output_record in &all_outputs {
        register_existing_output(
            state,
            output_record.output_id.clone(),
            output_record.connector_name.clone(),
            &output_record.output,
            output_record.mode,
            smithay::utils::Transform::Normal,
            output_record.location,
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
            location: (0, 0).into(),
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
            }
            DrmEvent::Error(error) => {
                info!(node = ?node, ?error, "tty preview drm device error");
                self.handle_tty_drm_node_changed(node);
            }
        }
    }

    fn handle_tty_session_activated(&mut self, seat_name: String) {
        info!(seat = %seat_name, "tty preview session activated; refreshing drm outputs");
        let needs_reinitialize = matches!(
            self.backend.as_ref(),
            Some(BackendState::Tty(backend)) if backend.drm.devices.iter().any(|device| {
                device.surfaces.is_empty() || device.surfaces.iter().any(|surface| surface.compositor.is_none())
            })
        );
        if needs_reinitialize {
            self.handle_tty_drm_changed();
        }
        self.schedule_tty_redraw();
    }

    fn handle_tty_session_paused(&mut self, seat_name: String) {
        info!(seat = %seat_name, "tty preview session paused");
        self.reset_tty_scanout_state();
    }

    pub(crate) fn handle_tty_drm_changed(&mut self) {
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
            assign_tty_output_locations(&mut next_outputs, &previous_outputs);

            let stale = previous_outputs
                .iter()
                .filter(|previous| {
                    !next_outputs.iter().any(|next| next.output_id == previous.output_id)
                })
                .map(|output| (output.output.clone(), output.output_id.clone()))
                .collect::<Vec<_>>();

            let updates = next_outputs
                .iter()
                .filter_map(|output| {
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
                        output.location,
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

    fn handle_tty_drm_node_changed(&mut self, node: DrmNode) {
        let (stale_outputs, updates, seat_name) = {
            let Some(BackendState::Tty(backend)) = self.backend.as_mut() else {
                return;
            };
            let seat_name = match &backend.session {
                BackendSession::Tty(session) => session.seat_name.clone(),
                _ => return,
            };

            let previous_outputs = std::mem::take(&mut backend.outputs);
            reset_tty_device_scanout_state(backend, node);

            let mut next_outputs = Vec::new();
            for device in &backend.drm.devices {
                let Ok(mut outputs) = collect_connector_outputs(&seat_name, device) else {
                    continue;
                };
                next_outputs.append(&mut outputs);
            }
            assign_tty_output_locations(&mut next_outputs, &previous_outputs);

            let stale = previous_outputs
                .iter()
                .filter(|previous| {
                    previous.drm_node == Some(node)
                        && !next_outputs.iter().any(|next| next.output_id == previous.output_id)
                })
                .map(|output| (output.output.clone(), output.output_id.clone()))
                .collect::<Vec<_>>();

            let updates = next_outputs
                .iter()
                .filter_map(|output| {
                    if output.drm_node != Some(node) {
                        return None;
                    }
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
                        output.location,
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
            let _ = initialize_tty_device_surfaces(&seat_name, backend, node);
        }
        let _ = initialize_tty_renderer_state_for_node(self, node);
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

    pub(crate) fn schedule_tty_redraw(&mut self) {
        let Some(BackendState::Tty(backend)) = self.backend.as_mut() else {
            return;
        };
        if backend.redraw_pending {
            return;
        }
        backend.redraw_pending = true;

        let handle = self.event_loop.clone();
        handle.insert_idle(|state| {
            if let Some(BackendState::Tty(backend)) = state.backend.as_mut() {
                backend.redraw_pending = false;
            }
            state.try_tty_blank_frame();
        });
    }

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
