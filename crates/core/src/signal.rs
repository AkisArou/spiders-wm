use serde::{Deserialize, Serialize};

use crate::{OutputId, SeatId, WindowId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum WmSignal {
    EnsureSeat {
        seat_id: SeatId,
    },
    OutputSynced {
        output_id: OutputId,
        name: String,
        logical_width: u32,
        logical_height: u32,
    },
    OutputRemoved {
        output_id: OutputId,
    },
    HoveredWindowChanged {
        seat_id: SeatId,
        hovered_window_id: Option<WindowId>,
    },
    InteractedWindowChanged {
        seat_id: SeatId,
        interacted_window_id: Option<WindowId>,
    },
    WindowIdentityChanged {
        window_id: WindowId,
        title: Option<String>,
        app_id: Option<String>,
        class: Option<String>,
        instance: Option<String>,
    },
    WindowMappedChanged {
        window_id: WindowId,
        mapped: bool,
    },
}
