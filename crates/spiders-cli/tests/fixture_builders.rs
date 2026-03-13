use std::fs;

use spiders_compositor::{
    BootstrapScenario, BootstrapScript, BootstrapTranscript, StartupRegistration,
};
use spiders_shared::ids::OutputId;

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bootstrap-events")
        .join(name)
}

#[test]
fn bootstrap_event_fixture_matches_builder_output() {
    let expected = fs::read_to_string(fixture("success.json")).unwrap();
    let script = BootstrapScript::Events(
        BootstrapScenario::new()
            .register_seat("seat-1", true)
            .register_window_surface(
                "window-w1",
                "bootstrap-window",
                Some(OutputId::from("bootstrap-output")),
            )
            .register_popup_surface(
                "popup-1",
                Some(OutputId::from("bootstrap-output")),
                "window-w1",
            )
            .unmap_surface("popup-1"),
    );

    let expected: serde_json::Value = serde_json::from_str(&expected).unwrap();
    let actual: serde_json::Value = serde_json::from_str(&script.to_json_pretty()).unwrap();

    assert_eq!(actual, expected);
}

#[test]
fn bootstrap_transcript_fixture_matches_builder_output() {
    let expected = fs::read_to_string(fixture("transcript-success.json")).unwrap();
    let script = BootstrapScript::Transcript(BootstrapTranscript::new(
        StartupRegistration {
            seats: vec!["seat-0".into(), "seat-1".into()],
            outputs: vec![OutputId::from("bootstrap-output")],
            active_seat: Some("seat-1".into()),
            active_output: Some(OutputId::from("bootstrap-output")),
        },
        BootstrapScenario::new()
            .register_seat("seat-1", true)
            .register_window_surface(
                "window-w1",
                "bootstrap-window",
                Some(OutputId::from("bootstrap-output")),
            )
            .register_popup_surface(
                "popup-1",
                Some(OutputId::from("bootstrap-output")),
                "window-w1",
            ),
    ));

    let expected: serde_json::Value = serde_json::from_str(&expected).unwrap();
    let actual: serde_json::Value = serde_json::from_str(&script.to_json_pretty()).unwrap();

    assert_eq!(actual, expected);
}
