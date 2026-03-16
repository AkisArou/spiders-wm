use spiders_effects::{
    EffectPseudoState, EffectStyle, EffectStyleSheet, EffectTarget, EffectsCssParseError,
    TitlebarEffects, WindowEffects, compute_effect_style, parse_effect_stylesheet,
};
use spiders_shared::ids::WindowId;
use spiders_shared::wm::{StateSnapshot, WindowSnapshot, WorkspaceSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowEffectsState {
    pub window_id: WindowId,
    pub style: EffectStyle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowDecorationPolicy {
    pub decorations_visible: bool,
    pub titlebar_visible: bool,
    pub window_style: WindowEffects,
    pub titlebar_style: TitlebarEffects,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EffectsRuntimeState {
    pub stylesheet: Option<EffectStyleSheet>,
    pub windows: Vec<WindowEffectsState>,
}

impl EffectsRuntimeState {
    pub fn from_stylesheet(source: &str) -> Result<Self, EffectsCssParseError> {
        let stylesheet = if source.trim().is_empty() {
            None
        } else {
            Some(parse_effect_stylesheet(source)?)
        };

        Ok(Self {
            stylesheet,
            windows: Vec::new(),
        })
    }

    pub fn recompute_for_workspace(
        &mut self,
        state: &StateSnapshot,
        workspace: &WorkspaceSnapshot,
    ) {
        let Some(stylesheet) = self.stylesheet.as_ref() else {
            self.windows.clear();
            return;
        };

        self.windows = state
            .windows_for_workspace(workspace)
            .into_iter()
            .map(|window| WindowEffectsState {
                window_id: window.id.clone(),
                style: resolve_window_effect_style(stylesheet, &window),
            })
            .collect();
    }

    pub fn window_style(&self, window_id: &WindowId) -> Option<&EffectStyle> {
        self.windows
            .iter()
            .find(|entry| &entry.window_id == window_id)
            .map(|entry| &entry.style)
    }

    pub fn window_decoration_policy(&self, window_id: &WindowId) -> Option<WindowDecorationPolicy> {
        self.window_style(window_id)
            .map(window_decoration_policy_for_style)
    }
}

pub fn resolve_window_effect_style(
    stylesheet: &EffectStyleSheet,
    window: &WindowSnapshot,
) -> EffectStyle {
    compute_effect_style(stylesheet, EffectTarget::Window(window), &[])
        .unwrap_or_else(|_| EffectStyle::default())
}

pub fn decoration_visible(style: &EffectStyle) -> bool {
    !matches!(
        style.window.appearance,
        Some(spiders_effects::Appearance::None)
    )
}

pub fn titlebar_visible(style: &EffectStyle) -> bool {
    decoration_visible(style)
}

pub fn window_decoration_policy_for_style(style: &EffectStyle) -> WindowDecorationPolicy {
    WindowDecorationPolicy {
        decorations_visible: decoration_visible(style),
        titlebar_visible: titlebar_visible(style),
        window_style: style.window.clone(),
        titlebar_style: style.titlebar.clone(),
    }
}

pub fn workspace_transition_states(
    _state: &StateSnapshot,
    _workspace: &WorkspaceSnapshot,
) -> Vec<EffectPseudoState> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use spiders_effects::Appearance;
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::{LayoutRef, OutputSnapshot, OutputTransform, ShellKind};

    use super::*;

    fn workspace() -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            id: WorkspaceId::from("ws-1"),
            name: "1".into(),
            output_id: Some(OutputId::from("out-1")),
            active_tags: vec!["1".into()],
            focused: true,
            visible: true,
            effective_layout: Some(LayoutRef {
                name: "master-stack".into(),
            }),
        }
    }

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![workspace()],
            windows: vec![WindowSnapshot {
                id: WindowId::from("win-1"),
                shell: ShellKind::XdgToplevel,
                app_id: Some("foot".into()),
                title: Some("shell".into()),
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                floating: false,
                floating_rect: None,
                fullscreen: false,
                focused: true,
                urgent: false,
                output_id: Some(OutputId::from("out-1")),
                workspace_id: Some(WorkspaceId::from("ws-1")),
                tags: vec!["1".into()],
            }],
            visible_window_ids: vec![WindowId::from("win-1")],
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn recomputes_window_effect_styles_for_current_workspace() {
        let mut effects = EffectsRuntimeState::from_stylesheet(
            r#"
                window[app_id="foot"] { appearance: none; }
                window::titlebar { background: #111; }
            "#,
        )
        .unwrap();

        let snapshot = state();
        effects.recompute_for_workspace(&snapshot, &workspace());

        let style = effects.window_style(&WindowId::from("win-1")).unwrap();
        assert_eq!(style.window.appearance, Some(Appearance::None));
        assert_eq!(style.titlebar.background.as_deref(), Some("#111"));
    }

    #[test]
    fn decoration_visibility_tracks_appearance_none() {
        let mut style = EffectStyle::default();
        style.window.appearance = Some(Appearance::None);

        assert!(!decoration_visible(&style));
        assert!(!titlebar_visible(&style));
    }

    #[test]
    fn empty_stylesheet_clears_cached_window_effects() {
        let mut effects = EffectsRuntimeState::from_stylesheet("").unwrap();
        effects.windows.push(WindowEffectsState {
            window_id: WindowId::from("win-1"),
            style: EffectStyle::default(),
        });

        effects.recompute_for_workspace(&state(), &workspace());

        assert!(effects.windows.is_empty());
    }

    #[test]
    fn computes_window_decoration_policy_from_cached_style() {
        let mut effects = EffectsRuntimeState::from_stylesheet(
            r#"
                window { appearance: none; }
                window::titlebar { background: #111; }
            "#,
        )
        .unwrap();

        effects.recompute_for_workspace(&state(), &workspace());
        let policy = effects
            .window_decoration_policy(&WindowId::from("win-1"))
            .unwrap();

        assert!(!policy.decorations_visible);
        assert!(!policy.titlebar_visible);
        assert_eq!(policy.titlebar_style.background.as_deref(), Some("#111"));
    }

    #[test]
    fn parses_window_border_effect_properties() {
        let mut effects = EffectsRuntimeState::from_stylesheet(
            r#"
                workspace { transition-property: transform, opacity; }
                window { border-width: 2px; border-color: #222222; opacity: 0.94; }
                window:focused { border-color: #285577; opacity: 1; }
            "#,
        )
        .unwrap();

        effects.recompute_for_workspace(&state(), &workspace());
        let style = effects.window_style(&WindowId::from("win-1")).unwrap();

        assert_eq!(style.window.border_width.as_deref(), Some("2px"));
        assert_eq!(style.window.border_color.as_deref(), Some("#285577"));
        assert_eq!(style.window.opacity.as_deref(), Some("1"));
    }
}
