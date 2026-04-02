use tracing::debug;

use crate::state::SpidersWm;

impl SpidersWm {
    pub(crate) fn managed_window_debug_summary(&self) -> Vec<String> {
        self.managed_windows()
            .iter()
            .map(|record| {
                format!(
                    "{}:mapped={}:closing={}:snapshot={}:pending_configures={}",
                    record.id.0,
                    record.mapped,
                    self.window_is_closing(&record.id),
                    record.frame_sync.has_close_snapshot(),
                    record.frame_sync.has_pending_configures(),
                )
            })
            .collect()
    }

    pub(crate) fn log_managed_window_state(&self, reason: &str) {
        debug!(
            reason,
            windows = ?self.managed_window_debug_summary(),
            closing_overlays = self.frame_sync.overlay_count(),
            focused = ?self.focused_surface.as_ref().and_then(|surface| self.window_id_for_surface(surface)),
            "wm managed window state"
        );
    }
}
