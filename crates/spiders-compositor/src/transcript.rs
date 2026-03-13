pub use spiders_runtime::BootstrapTranscript;

#[cfg(test)]
mod tests {
    use spiders_shared::ids::OutputId;

    use super::*;
    use crate::scenario::BootstrapScenario;

    #[test]
    fn transcript_round_trips_through_json() {
        let transcript = BootstrapTranscript::new(
            spiders_runtime::StartupRegistration {
                seats: vec!["seat-0".into(), "seat-1".into()],
                outputs: vec![OutputId::from("out-1")],
                active_seat: Some("seat-1".into()),
                active_output: Some(OutputId::from("out-1")),
            },
            BootstrapScenario::new()
                .register_seat("seat-1", true)
                .register_output("out-1", true),
        );

        let json = transcript.to_json_pretty();
        let parsed = BootstrapTranscript::from_json_str(&json).unwrap();

        assert_eq!(parsed, transcript);
    }
}
