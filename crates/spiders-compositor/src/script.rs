pub use spiders_runtime::{BootstrapScript, BootstrapScriptKind};

#[cfg(test)]
mod tests {
    use spiders_shared::ids::OutputId;

    use super::*;
    use crate::scenario::BootstrapScenario;
    use crate::transcript::BootstrapTranscript;

    #[test]
    fn script_round_trips_event_arrays() {
        let script = BootstrapScript::Events(
            BootstrapScenario::new()
                .register_seat("seat-1", true)
                .register_output("out-1", true),
        );

        let parsed = BootstrapScript::from_json_str(&script.to_json_pretty()).unwrap();

        assert_eq!(parsed, script);
        assert_eq!(parsed.kind(), BootstrapScriptKind::Events);
        assert!(parsed.startup().is_none());
        assert_eq!(parsed.scenario().events().len(), 2);
    }

    #[test]
    fn script_round_trips_transcripts() {
        let script = BootstrapScript::Transcript(BootstrapTranscript::new(
            spiders_runtime::StartupRegistration {
                seats: vec!["seat-0".into(), "seat-1".into()],
                outputs: vec![OutputId::from("out-1")],
                active_seat: Some("seat-1".into()),
                active_output: Some(OutputId::from("out-1")),
            },
            BootstrapScenario::new().register_output("out-1", true),
        ));

        let parsed = BootstrapScript::from_json_str(&script.to_json_pretty()).unwrap();

        assert_eq!(parsed, script);
        assert_eq!(parsed.kind(), BootstrapScriptKind::Transcript);
        assert_eq!(
            parsed
                .startup()
                .and_then(|startup| startup.active_seat.as_deref()),
            Some("seat-1")
        );
        assert_eq!(parsed.scenario().events().len(), 1);
    }
}
