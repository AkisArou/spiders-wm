# Bootstrap Event Scripts

`spiders-cli bootstrap-trace` can replay a JSON array of typed bootstrap events
with `--events <path>`.

This is a backend-agnostic way to simulate startup discovery and teardown before
any real compositor backend loop exists.

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
```

Reusable examples live in:

- `crates/spiders-cli/tests/fixtures/bootstrap-events/success.json`
- `crates/spiders-cli/tests/fixtures/bootstrap-events/failure.json`

## Notes

- Event names use kebab-case.
- Successful runs return bootstrap diagnostics, startup registration, topology id
  lists, and applied event counts.
- Failed runs return a structured error with the failed event and partial trace.
