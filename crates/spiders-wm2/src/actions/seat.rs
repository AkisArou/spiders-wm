use crate::model::wm::WmModel;
use crate::model::{SeatId, WindowId};

pub fn ensure_seat(model: &mut WmModel, seat_id: impl Into<SeatId>) -> SeatId {
    let seat_id = seat_id.into();
    model.upsert_seat(seat_id.clone());
    seat_id
}

pub fn sync_focused_window(
    model: &mut WmModel,
    seat_id: impl Into<SeatId>,
    focused_window_id: Option<WindowId>,
) -> Option<WindowId> {
    let seat_id = ensure_seat(model, seat_id);
    let focused_window_id = focused_window_id.filter(|window_id| model.windows.contains_key(window_id));
    model.set_seat_focused_window(seat_id, focused_window_id);
    focused_window_id
}

pub fn sync_hovered_window(
    model: &mut WmModel,
    seat_id: impl Into<SeatId>,
    hovered_window_id: Option<WindowId>,
) -> Option<WindowId> {
    let seat_id = ensure_seat(model, seat_id);
    let hovered_window_id = hovered_window_id.filter(|window_id| model.windows.contains_key(window_id));
    model.set_seat_hovered_window(seat_id, hovered_window_id);
    hovered_window_id
}

pub fn sync_interacted_window(
    model: &mut WmModel,
    seat_id: impl Into<SeatId>,
    interacted_window_id: Option<WindowId>,
) -> Option<WindowId> {
    let seat_id = ensure_seat(model, seat_id);
    let interacted_window_id = interacted_window_id.filter(|window_id| model.windows.contains_key(window_id));
    model.set_seat_interacted_window(seat_id, interacted_window_id);
    interacted_window_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensuring_seat_creates_once() {
        let mut model = WmModel::default();

        let seat_id = ensure_seat(&mut model, "winit");
        let seat_id_again = ensure_seat(&mut model, "winit");

        assert_eq!(seat_id, SeatId("winit".to_string()));
        assert_eq!(seat_id_again, SeatId("winit".to_string()));
        assert_eq!(model.seats.len(), 1);
    }

    #[test]
    fn syncing_seat_focus_tracks_known_window() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(1), None, None);

        let focused = sync_focused_window(&mut model, "winit", Some(WindowId(1)));

        assert_eq!(focused, Some(WindowId(1)));
        assert_eq!(
            model.seats.get(&SeatId("winit".to_string())).and_then(|seat| seat.focused_window_id),
            Some(WindowId(1))
        );
    }

    #[test]
    fn syncing_seat_focus_clears_unknown_window() {
        let mut model = WmModel::default();

        let focused = sync_focused_window(&mut model, "winit", Some(WindowId(9)));

        assert_eq!(focused, None);
        assert_eq!(
            model.seats.get(&SeatId("winit".to_string())).and_then(|seat| seat.focused_window_id),
            None
        );
    }

    #[test]
    fn syncing_hovered_window_tracks_known_window() {
        let mut model = WmModel::default();
        model.insert_window(WindowId(2), None, None);

        let hovered = sync_hovered_window(&mut model, "winit", Some(WindowId(2)));

        assert_eq!(hovered, Some(WindowId(2)));
        assert_eq!(
            model.seats.get(&SeatId("winit".to_string())).and_then(|seat| seat.hovered_window_id),
            Some(WindowId(2))
        );
    }

    #[test]
    fn syncing_interacted_window_clears_unknown_window() {
        let mut model = WmModel::default();

        let interacted = sync_interacted_window(&mut model, "winit", Some(WindowId(8)));

        assert_eq!(interacted, None);
        assert_eq!(
            model.seats.get(&SeatId("winit".to_string())).and_then(|seat| seat.interacted_window_id),
            None
        );
    }
}