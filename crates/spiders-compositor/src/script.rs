use crate::app::{BootstrapEvent, StartupRegistration};
use crate::scenario::BootstrapScenario;
use crate::transcript::BootstrapTranscript;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapScriptKind {
    Events,
    Transcript,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapScript {
    Events(BootstrapScenario),
    Transcript(BootstrapTranscript),
}

impl BootstrapScript {
    pub fn kind(&self) -> BootstrapScriptKind {
        match self {
            Self::Events(_) => BootstrapScriptKind::Events,
            Self::Transcript(_) => BootstrapScriptKind::Transcript,
        }
    }

    pub fn startup(&self) -> Option<&StartupRegistration> {
        match self {
            Self::Events(_) => None,
            Self::Transcript(transcript) => Some(&transcript.startup),
        }
    }

    pub fn scenario(&self) -> &BootstrapScenario {
        match self {
            Self::Events(scenario) => scenario,
            Self::Transcript(transcript) => &transcript.scenario,
        }
    }

    pub fn into_parts(self) -> (Option<StartupRegistration>, BootstrapScenario) {
        match self {
            Self::Events(scenario) => (None, scenario),
            Self::Transcript(transcript) => (Some(transcript.startup), transcript.scenario),
        }
    }

    pub fn to_json_pretty(&self) -> String {
        match self {
            Self::Events(scenario) => scenario.to_json_pretty(),
            Self::Transcript(transcript) => transcript.to_json_pretty(),
        }
    }

    pub fn from_json_str(json: &str) -> Result<Self, serde_json::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum BootstrapScriptRepr {
            Transcript(BootstrapTranscript),
            Events(Vec<BootstrapEvent>),
        }

        match serde_json::from_str::<BootstrapScriptRepr>(json)? {
            BootstrapScriptRepr::Transcript(transcript) => Ok(Self::Transcript(transcript)),
            BootstrapScriptRepr::Events(events) => {
                Ok(Self::Events(BootstrapScenario::from_events(events)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::OutputId;

    use super::*;

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
            StartupRegistration {
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
