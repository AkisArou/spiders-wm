pub use spiders_runtime::BootstrapScenario;

#[cfg(test)]
mod tests {
    use spiders_runtime::BootstrapEvent;
    use spiders_shared::ids::OutputId;

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

    #[test]
    fn scenario_round_trips_through_json() {
        let scenario = BootstrapScenario::new()
            .register_seat("seat-1", true)
            .register_window_surface("window-w1", "w1", Some(OutputId::from("out-1")))
            .register_popup_surface("popup-1", Some(OutputId::from("out-1")), "window-w1");

        let json = scenario.to_json_pretty();
        let parsed = BootstrapScenario::from_json_str(&json).unwrap();

        assert_eq!(parsed, scenario);
    }
}
