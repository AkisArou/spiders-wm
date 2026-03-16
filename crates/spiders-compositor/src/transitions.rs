#[cfg(feature = "smithay-winit")]
mod imp {
    use smithay::backend::renderer::element::texture::TextureBuffer;
    use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
    use smithay::utils::Monotonic;
    use spiders_shared::ids::WindowId;
    use spiders_shared::layout::LayoutRect;

    use crate::smithay_state::SmithayWindowRenderSnapshot;

    #[derive(Debug, Clone)]
    pub struct TransitionTextureSnapshot {
        pub buffer: TextureBuffer<GlesTexture>,
        pub render_snapshot: SmithayWindowRenderSnapshot,
        pub logical_size: (f64, f64),
    }

    #[derive(Debug, Clone)]
    pub struct SceneTextureSnapshot {
        pub buffer: TextureBuffer<GlesTexture>,
        pub logical_size: (f64, f64),
    }

    #[derive(Debug, Clone)]
    pub struct ResizeTransition {
        pub window_id: WindowId,
        pub from: TransitionTextureSnapshot,
        pub target_logical_size: (i32, i32),
        pub started_at: std::time::Duration,
        pub awaiting_target_commit: bool,
        pub close_driven: bool,
    }

    #[derive(Debug, Clone)]
    pub struct ClosingWindowTransition {
        pub window_id: WindowId,
        pub snapshot: TransitionTextureSnapshot,
        pub started_at: std::time::Duration,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OpeningWindowTransition {
        pub window_id: WindowId,
        pub first_seen_at: std::time::Duration,
    }

    #[derive(Debug, Clone)]
    pub struct SceneTransition {
        pub snapshot: SceneTextureSnapshot,
        pub started_at: std::time::Duration,
        pub awaiting_layout_change: bool,
        pub awaiting_settle: bool,
        pub captured_layouts: Vec<(WindowId, (i32, i32, i32, i32))>,
        pub affected_windows: Vec<WindowId>,
        pub blocked_windows: Vec<WindowId>,
        pub frozen_rect: LayoutRect,
    }

    impl ResizeTransition {
        pub fn new(
            window_id: WindowId,
            from: TransitionTextureSnapshot,
            target_logical_size: (i32, i32),
            started_at: std::time::Duration,
            close_driven: bool,
        ) -> Self {
            Self {
                window_id,
                from,
                target_logical_size,
                started_at,
                awaiting_target_commit: true,
                close_driven,
            }
        }
    }

    impl ClosingWindowTransition {
        pub fn new(
            window_id: WindowId,
            snapshot: TransitionTextureSnapshot,
            started_at: std::time::Duration,
        ) -> Self {
            Self {
                window_id,
                snapshot,
                started_at,
            }
        }
    }

    impl OpeningWindowTransition {
        pub fn new(window_id: WindowId, first_seen_at: std::time::Duration) -> Self {
            Self {
                window_id,
                first_seen_at,
            }
        }
    }

    impl SceneTransition {
        pub fn new(
            snapshot: SceneTextureSnapshot,
            started_at: std::time::Duration,
            captured_layouts: Vec<(WindowId, (i32, i32, i32, i32))>,
            affected_windows: Vec<WindowId>,
            frozen_rect: LayoutRect,
        ) -> Self {
            Self {
                snapshot,
                started_at,
                awaiting_layout_change: true,
                awaiting_settle: true,
                captured_layouts,
                blocked_windows: affected_windows.clone(),
                affected_windows,
                frozen_rect,
            }
        }
    }

    pub fn now(clock: &smithay::utils::Clock<Monotonic>) -> std::time::Duration {
        clock.now().into()
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::*;
