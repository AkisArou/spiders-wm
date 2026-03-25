use super::{ClosePathQueue, ClosingWindow, Wm2RenderElements};

#[derive(Default)]
pub struct FrameSyncState {
    closing_windows: Vec<ClosingWindow>,
    close_path_queue: ClosePathQueue,
}

impl FrameSyncState {
    pub fn push_closing_window(&mut self, window: ClosingWindow) {
        self.closing_windows.push(window);
    }

    pub fn has_active_closing(&self) -> bool {
        !self.closing_windows.is_empty()
    }

    pub fn queue_relayout(&mut self) {
        self.close_path_queue.queue_relayout();
    }

    pub fn take_queued_relayout(&mut self) -> bool {
        self.close_path_queue.take_relayout()
    }

    pub fn advance_closing_windows(&mut self) {
        self.closing_windows.retain(|window| !window.is_finished());
    }

    pub fn render_elements(&self) -> Vec<Wm2RenderElements> {
        self.closing_windows
            .iter()
            .map(ClosingWindow::render_element)
            .collect()
    }
}