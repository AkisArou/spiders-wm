use std::time::Duration;

use smithay::input::Seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::SERIAL_COUNTER;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};

use crate::state::SpidersWm;

const XDG_ACTIVATION_TOKEN_TIMEOUT: Duration = Duration::from_secs(10);

impl XdgActivationHandler for SpidersWm {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.activation_state
    }

    fn token_created(&mut self, _token: XdgActivationToken, data: XdgActivationTokenData) -> bool {
        let Some((serial, seat)) = data.serial.as_ref() else {
            return false;
        };
        let Some(seat) = Seat::<SpidersWm>::from_resource(seat) else {
            return false;
        };

        activation_serial_is_valid(&seat, *serial)
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        if pending_activation_is_valid(&token_data) {
            match activation_request_outcome(self.window_id_for_surface(&surface).is_some()) {
                ActivationRequestOutcome::FocusMappedWindow => {
                    self.set_focus(Some(surface.clone()), SERIAL_COUNTER.next_serial());
                }
                ActivationRequestOutcome::StorePendingRequest => {
                    self.pending_activation_requests
                        .retain(|(pending_surface, _)| pending_surface != &surface);
                    self.pending_activation_requests.push((surface.clone(), token_data));
                }
            }
        }

        self.activation_state.remove_token(&token);
    }
}

pub(crate) fn activation_serial_is_valid(
    seat: &Seat<SpidersWm>,
    serial: smithay::utils::Serial,
) -> bool {
    let keyboard_valid = seat
        .get_keyboard()
        .and_then(|keyboard| keyboard.last_enter())
        .is_some_and(|last_enter| serial.is_no_older_than(&last_enter));
    let pointer_valid = seat
        .get_pointer()
        .and_then(|pointer| pointer.last_enter())
        .is_some_and(|last_enter| serial.is_no_older_than(&last_enter));

    activation_serial_matches(keyboard_valid, pointer_valid)
}

pub(crate) fn pending_activation_is_valid(token_data: &XdgActivationTokenData) -> bool {
    token_data.timestamp.elapsed() < XDG_ACTIVATION_TOKEN_TIMEOUT
}

fn activation_request_outcome(surface_is_mapped: bool) -> ActivationRequestOutcome {
    if surface_is_mapped {
        ActivationRequestOutcome::FocusMappedWindow
    } else {
        ActivationRequestOutcome::StorePendingRequest
    }
}

fn activation_serial_matches(
    serial_is_no_older_than_keyboard_enter: bool,
    serial_is_no_older_than_pointer_enter: bool,
) -> bool {
    serial_is_no_older_than_keyboard_enter || serial_is_no_older_than_pointer_enter
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivationRequestOutcome {
    FocusMappedWindow,
    StorePendingRequest,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn pending_activation_is_valid_for_recent_token() {
        assert!(pending_activation_is_valid(&XdgActivationTokenData::default()));
    }

    #[test]
    fn pending_activation_is_invalid_for_old_token() {
        let mut token_data = XdgActivationTokenData::default();
        token_data.timestamp =
            Instant::now() - XDG_ACTIVATION_TOKEN_TIMEOUT - Duration::from_secs(1);

        assert!(!pending_activation_is_valid(&token_data));
    }

    #[test]
    fn activation_request_outcome_focuses_mapped_windows() {
        assert_eq!(activation_request_outcome(true), ActivationRequestOutcome::FocusMappedWindow,);
    }

    #[test]
    fn activation_request_outcome_stores_unmapped_windows() {
        assert_eq!(
            activation_request_outcome(false),
            ActivationRequestOutcome::StorePendingRequest,
        );
    }

    #[test]
    fn activation_serial_matches_keyboard_or_pointer_enter() {
        assert!(activation_serial_matches(true, false));
        assert!(activation_serial_matches(false, true));
        assert!(activation_serial_matches(true, true));
        assert!(!activation_serial_matches(false, false));
    }
}
