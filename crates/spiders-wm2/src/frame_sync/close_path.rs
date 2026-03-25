//! Close path relayout coordination.
//!
//! For the non-animated path we only need to remember whether one follow-up relayout
//! should run after the current frame-sync transition finishes.

/// Deduplicating relayout latch for overlapping open/close requests.
pub struct ClosePathQueue {
    relayout_queued: bool,
}

impl ClosePathQueue {
    /// Creates a new close path queue.
    pub fn new() -> Self {
        Self {
            relayout_queued: false,
        }
    }

    /// Queues a relayout operation.
    pub fn queue_relayout(&mut self) {
        self.relayout_queued = true;
    }

    /// Consumes the queued relayout request.
    pub fn take_relayout(&mut self) -> bool {
        std::mem::take(&mut self.relayout_queued)
    }

}

impl Default for ClosePathQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_path_queue_deduplicates_relayout() {
        let mut queue = ClosePathQueue::new();
        queue.queue_relayout();
        queue.queue_relayout();
        queue.queue_relayout();

        assert!(queue.take_relayout());
        assert!(!queue.take_relayout());
    }
}
