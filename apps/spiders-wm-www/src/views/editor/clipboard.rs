use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::editor_host::copy_text_to_clipboard;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyFeedback {
    Idle,
    Copied,
    Failed,
}

impl CopyFeedback {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "copy",
            Self::Copied => "copied",
            Self::Failed => "copy failed",
        }
    }
}

pub fn copy_buffer_to_clipboard(contents: String, feedback: RwSignal<CopyFeedback>) {
    feedback.set(CopyFeedback::Idle);

    spawn_local(async move {
        let next_feedback = match copy_text_to_clipboard(&contents).await {
            Ok(()) => CopyFeedback::Copied,
            Err(_) => CopyFeedback::Failed,
        };

        feedback.set(next_feedback);
    });
}
