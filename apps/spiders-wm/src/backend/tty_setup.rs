#[cfg(feature = "tty-preview")]
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
#[cfg(all(feature = "tty-preview", feature = "libseat"))]
use smithay::backend::session::libseat::LibSeatSession;
#[cfg(feature = "tty-preview")]
use smithay::backend::udev::{UdevBackend, UdevEvent, all_gpus, primary_gpu};
#[cfg(feature = "tty-preview")]
use smithay::reexports::calloop::EventLoop;
#[cfg(feature = "tty-preview")]
use tracing::info;

#[cfg(feature = "tty-preview")]
use crate::backend::session::{TtySessionHandle, TtySessionState};
#[cfg(feature = "tty-preview")]
use crate::state::SpidersWm;

#[cfg(feature = "tty-preview")]
pub(crate) fn init_tty_preview_sources(
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
        event_loop.handle().insert_source(backend, |event, _, state| {
            state.process_input_event(event);
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
fn log_tty_preview_gpu_candidates(
    session: &TtySessionState,
) -> Result<(), Box<dyn std::error::Error>> {
    let seat_name = &session.seat_name;
    let primary = primary_gpu(seat_name)?;
    let gpus = all_gpus(seat_name)?;
    info!(seat = seat_name, primary_gpu = ?primary, gpu_count = gpus.len(), "tty preview enumerated gpus");
    for gpu in gpus {
        info!(seat = seat_name, path = %gpu.display(), "tty preview gpu candidate");
    }

    Ok(())
}
