use std::time::Instant;

use spiders_scene::{
    Animation, CompiledKeyframesRule, ComputedStyle, MotionContext, ResolvedMotion, Transition,
};
use spiders_tree::WindowId;

use crate::backend::transient::WindowMotionState;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MotionStyleScope {
    Layout,
    Titlebar,
}

impl RiverBackendState {
    pub(super) fn begin_motion_frame(&mut self) {
        self.transient.motion_frame_time = Some(Instant::now());
        self.transient.motion_has_active_animations = false;
        self.transient.motion_active_windows.clear();
    }

    pub(super) fn finish_motion_frame(&mut self) -> bool {
        self.transient.motion_frame_time = None;
        self.transient.motion_has_active_animations
    }

    pub(super) fn clear_window_motion_state(&mut self, window_id: &WindowId) {
        self.transient.motion_state.remove(window_id);
    }

    pub(super) fn resolve_motion(
        &mut self,
        window_id: &WindowId,
        scope: MotionStyleScope,
        style: Option<&ComputedStyle>,
        keyframes: &[CompiledKeyframesRule],
        width: f32,
        height: f32,
    ) -> ResolvedMotion {
        let now = self
            .transient
            .motion_frame_time
            .unwrap_or_else(Instant::now);

        let window_state = self
            .transient
            .motion_state
            .entry(window_id.clone())
            .or_insert_with(WindowMotionState::default);
        let track = match scope {
            MotionStyleScope::Layout => &mut window_state.layout,
            MotionStyleScope::Titlebar => &mut window_state.titlebar,
        };

        let context = MotionContext { width, height };
        let animation_runtime = Animation::new(style, keyframes);
        let transition_runtime = Transition::new(style);
        let animation = animation_runtime.apply(track, context, now);
        let transition = transition_runtime.apply(track, context, animation.active, now);
        let motion = ResolvedMotion {
            opacity: if animation.active.opacity {
                animation.motion.opacity
            } else {
                transition.motion.opacity
            },
            transform: if animation.active.transform {
                animation.motion.transform
            } else {
                transition.motion.transform
            },
        };
        let active = animation.active.any() || transition.active.any();
        if active {
            self.transient.motion_has_active_animations = true;
            self.transient
                .motion_active_windows
                .insert(window_id.clone());
        }
        motion
    }
}
