use spiders_shared::ids::{OutputId, WindowId};

use crate::app::BootstrapEvent;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BootstrapScenario {
    events: Vec<BootstrapEvent>,
}

impl BootstrapScenario {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn from_events(events: Vec<BootstrapEvent>) -> Self {
        Self { events }
    }

    pub fn events(&self) -> &[BootstrapEvent] {
        &self.events
    }

    pub fn into_events(self) -> Vec<BootstrapEvent> {
        self.events
    }

    pub fn register_seat(mut self, seat_name: impl Into<String>, active: bool) -> Self {
        self.events.push(BootstrapEvent::RegisterSeat {
            seat_name: seat_name.into(),
            active,
        });
        self
    }

    pub fn register_output(mut self, output_id: impl Into<OutputId>, active: bool) -> Self {
        self.events.push(BootstrapEvent::RegisterOutput {
            output_id: output_id.into(),
            active,
        });
        self
    }

    pub fn register_window_surface(
        mut self,
        surface_id: impl Into<String>,
        window_id: impl Into<WindowId>,
        output_id: Option<OutputId>,
    ) -> Self {
        self.events.push(BootstrapEvent::RegisterWindowSurface {
            surface_id: surface_id.into(),
            window_id: window_id.into(),
            output_id,
        });
        self
    }

    pub fn register_popup_surface(
        mut self,
        surface_id: impl Into<String>,
        output_id: Option<OutputId>,
        parent_surface_id: impl Into<String>,
    ) -> Self {
        self.events.push(BootstrapEvent::RegisterPopupSurface {
            surface_id: surface_id.into(),
            output_id,
            parent_surface_id: parent_surface_id.into(),
        });
        self
    }

    pub fn register_layer_surface(
        mut self,
        surface_id: impl Into<String>,
        output_id: impl Into<OutputId>,
    ) -> Self {
        self.events.push(BootstrapEvent::RegisterLayerSurface {
            surface_id: surface_id.into(),
            output_id: output_id.into(),
        });
        self
    }

    pub fn register_unmanaged_surface(mut self, surface_id: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::RegisterUnmanagedSurface {
            surface_id: surface_id.into(),
        });
        self
    }

    pub fn move_surface_to_output(
        mut self,
        surface_id: impl Into<String>,
        output_id: impl Into<OutputId>,
    ) -> Self {
        self.events.push(BootstrapEvent::MoveSurfaceToOutput {
            surface_id: surface_id.into(),
            output_id: output_id.into(),
        });
        self
    }

    pub fn unmap_surface(mut self, surface_id: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::UnmapSurface {
            surface_id: surface_id.into(),
        });
        self
    }

    pub fn remove_window_surface(mut self, window_id: impl Into<WindowId>) -> Self {
        self.events.push(BootstrapEvent::RemoveWindowSurface {
            window_id: window_id.into(),
        });
        self
    }

    pub fn remove_surface(mut self, surface_id: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::RemoveSurface {
            surface_id: surface_id.into(),
        });
        self
    }

    pub fn remove_output(mut self, output_id: impl Into<OutputId>) -> Self {
        self.events.push(BootstrapEvent::RemoveOutput {
            output_id: output_id.into(),
        });
        self
    }

    pub fn remove_seat(mut self, seat_name: impl Into<String>) -> Self {
        self.events.push(BootstrapEvent::RemoveSeat {
            seat_name: seat_name.into(),
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_builder_collects_ordered_events() {
        let scenario = BootstrapScenario::new()
            .register_seat("seat-1", true)
            .register_window_surface("window-w1", "w1", Some(OutputId::from("out-1")))
            .register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1")
            .unmap_surface("popup-1")
            .remove_window_surface("w1");

        assert_eq!(scenario.events().len(), 5);
        assert!(matches!(
            scenario.events()[0],
            BootstrapEvent::RegisterSeat { .. }
        ));
        assert!(matches!(
            scenario.events()[4],
            BootstrapEvent::RemoveWindowSurface { .. }
        ));
    }
}
