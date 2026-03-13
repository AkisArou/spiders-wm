# Bootstrap Event Scripts

`spiders-cli bootstrap-trace` can replay either a JSON array of typed bootstrap
events or a richer bootstrap transcript.

This is a backend-agnostic way to simulate startup discovery and teardown before
any real compositor backend loop exists.

For the broader bootstrap/runtime architecture, see
`docs/spec/compositor-bootstrap.md`.

## Example

```json
[
  {
    "register-seat": {
      "seat_name": "seat-1",
      "active": true
    }
  },
  {
    "register-window-surface": {
      "surface_id": "window-w1",
      "window_id": "bootstrap-window",
      "output_id": "bootstrap-output"
    }
  },
  {
    "register-popup-surface": {
      "surface_id": "popup-1",
      "output_id": "bootstrap-output",
      "parent_surface_id": "window-w1"
    }
  },
  {
    "unmap-surface": {
      "surface_id": "popup-1"
    }
  }
]
```

## Usage

```bash
spiders-cli bootstrap-trace --json --events path/to/bootstrap-events.json
spiders-cli bootstrap-trace --json --transcript path/to/bootstrap-transcript.json
```

Reusable examples live in:

- `crates/spiders-cli/tests/fixtures/bootstrap-events/success.json`
- `crates/spiders-cli/tests/fixtures/bootstrap-events/failure.json`
- `crates/spiders-cli/tests/fixtures/bootstrap-events/transcript-success.json`

Those fixtures are now covered by typed builder tests in
`crates/spiders-cli/tests/fixture_builders.rs` so the JSON examples stay aligned
with the Rust bootstrap model.

Event-array scripts can also be parsed through `--events` when the file only
contains ordered bootstrap events. Transcript files include explicit startup
registration plus a nested scenario.

## Transcript Example

```json
{
  "startup": {
    "seats": ["seat-0", "seat-1"],
    "outputs": ["bootstrap-output"],
    "active_seat": "seat-1",
    "active_output": "bootstrap-output"
  },
  "scenario": {
    "events": [
      {
        "register-seat": {
          "seat_name": "seat-1",
          "active": true
        }
      }
    ]
  }
}
```

## Notes

- Event names use kebab-case.
- Transcript files reuse the same event payloads inside `scenario.events`.
- Successful runs return bootstrap diagnostics, startup registration, topology id
  lists, applied event counts, and the controller lifecycle phase.
- Failed runs return a structured error with the failed event and partial trace.

Future backend integration should translate compositor/backend notifications into
typed `BackendDiscoveryEvent` values first, then feed those into
`CompositorController` rather than reaching into runner or topology state
directly.

For initial synchronization, a backend adapter can also construct a
`BackendTopologySnapshot` and let the controller expand it into typed discovery
events, which keeps batch import policy on the compositor side.
