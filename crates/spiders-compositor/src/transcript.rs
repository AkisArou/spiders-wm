use crate::app::StartupRegistration;
use crate::scenario::BootstrapScenario;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BootstrapTranscript {
    pub startup: StartupRegistration,
    pub scenario: BootstrapScenario,
}

impl BootstrapTranscript {
    pub fn new(startup: StartupRegistration, scenario: BootstrapScenario) -> Self {
        Self { startup, scenario }
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap()
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::OutputId;

    use super::*;

    #[test]
    fn transcript_round_trips_through_json() {
        let transcript = BootstrapTranscript::new(
            StartupRegistration {
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
