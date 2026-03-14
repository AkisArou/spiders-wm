use spiders_shared::api::{CompositorEvent, FocusDirection};
use spiders_shared::ids::{OutputId, WindowId};
use spiders_shared::wm::{OutputSnapshot, StateSnapshot, WindowSnapshot};

use crate::topology::{
    CompositorTopologyState, LayerSurfaceMetadata, SurfaceRole, SurfaceState, TopologyError,
};
use crate::wm::{WmState, WmStateError};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DomainSessionError {
    #[error(transparent)]
    Wm(#[from] WmStateError),
    #[error(transparent)]
    Topology(#[from] TopologyError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct DomainUpdate {
    pub events: Vec<CompositorEvent>,
    pub recomputed_layout: bool,
    pub topology: CompositorTopologyState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DomainSession {
    wm: WmState,
    topology: CompositorTopologyState,
}

impl DomainSession {
    pub fn new(wm: WmState, topology: CompositorTopologyState) -> Self {
        Self { wm, topology }
    }

    pub fn wm(&self) -> &WmState {
        &self.wm
    }

    pub fn wm_mut(&mut self) -> &mut WmState {
        &mut self.wm
    }

    pub fn topology(&self) -> &CompositorTopologyState {
        &self.topology
    }

    pub fn state(&self) -> &StateSnapshot {
        self.wm.snapshot()
    }

    pub fn register_popup_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: Option<OutputId>,
        parent_surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.topology.register_surface(
            surface_id,
            SurfaceRole::Popup,
            output_id,
            Some(parent_surface_id.into()),
        )
    }

    pub fn register_layer_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
    ) -> Result<&SurfaceState, TopologyError> {
        self.register_layer_surface_with_metadata(
            surface_id,
            output_id,
            LayerSurfaceMetadata {
                namespace: String::new(),
                tier: crate::topology::LayerSurfaceTier::Background,
                keyboard_interactivity: crate::topology::LayerKeyboardInteractivity::None,
                exclusive_zone: crate::topology::LayerExclusiveZone::Neutral,
            },
        )
    }

    pub fn register_layer_surface_with_metadata(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
        metadata: LayerSurfaceMetadata,
    ) -> Result<&SurfaceState, TopologyError> {
        self.topology
            .register_layer_surface(surface_id, output_id, metadata)
    }

    pub fn register_unmanaged_surface(
        &mut self,
        surface_id: impl Into<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.topology
            .register_surface(surface_id, SurfaceRole::Unmanaged, None, None)
    }

    pub fn register_output_snapshot(&mut self, output: OutputSnapshot) {
        self.topology.register_output(output);
    }

    pub fn register_backend_output_snapshot(&mut self, output: OutputSnapshot) {
        self.register_output_snapshot(output);
    }

    pub fn register_output_by_id(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        let output = self
            .state()
            .output_by_id(output_id)
            .cloned()
            .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))?;
        self.topology.register_output(output);
        Ok(())
    }

    pub fn register_startup_seeded_output(
        &mut self,
        output_id: &OutputId,
    ) -> Result<(), TopologyError> {
        self.register_output_by_id(output_id)
    }

    pub fn unregister_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.topology.unregister_output(output_id)
    }

    pub fn unregister_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        self.topology.unregister_seat(seat_name)
    }

    pub fn unregister_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        self.topology.unregister_surface(surface_id)
    }

    pub fn unregister_window_surface(&mut self, window_id: &WindowId) -> Result<(), TopologyError> {
        self.topology.unregister_window_surface(window_id)
    }

    pub fn move_surface_to_output(
        &mut self,
        surface_id: &str,
        output_id: OutputId,
    ) -> Result<(), TopologyError> {
        self.topology
            .update_surface_attachment(surface_id, Some(output_id))
    }

    pub fn unmap_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        self.topology.unmap_surface(surface_id)
    }

    pub fn register_window_surface(
        &mut self,
        surface_id: impl Into<String>,
        window_id: WindowId,
        output_id: Option<OutputId>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.topology
            .map_window_surface(surface_id, window_id, output_id)
    }

    pub fn register_seat(&mut self, seat_name: impl Into<String>) -> &str {
        let seat = self.topology.register_seat(seat_name);
        &seat.name
    }

    pub fn activate_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        self.topology.activate_seat(seat_name)?;
        Ok(())
    }

    pub fn activate_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.topology.activate_output(output_id)?;
        Ok(())
    }

    pub fn focus_seat(
        &mut self,
        seat_name: &str,
        window_id: Option<WindowId>,
        output_id: Option<OutputId>,
    ) -> Result<(), TopologyError> {
        self.topology
            .focus_seat_window(seat_name, window_id, output_id)?;
        Ok(())
    }

    pub fn disable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.topology.disable_output(output_id)
    }

    pub fn enable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        self.topology.enable_output(output_id)
    }

    pub fn map_window(
        &mut self,
        window: WindowSnapshot,
    ) -> Result<DomainUpdate, DomainSessionError> {
        let output_id = window.output_id.clone();
        let window_id = window.id.clone();
        let event = self.wm.map_window(window);
        let surface_id = format!("window-{window_id}");
        self.topology
            .map_window_surface(surface_id, window_id, output_id)?;
        Ok(self.domain_update(vec![event], true))
    }

    pub fn focus_window(
        &mut self,
        window_id: &WindowId,
    ) -> Result<DomainUpdate, DomainSessionError> {
        let event = self.wm.focus_window(window_id)?;
        self.synchronize_topology_focus()?;
        Ok(self.domain_update(vec![event], false))
    }

    pub fn destroy_window(
        &mut self,
        window_id: &WindowId,
    ) -> Result<DomainUpdate, DomainSessionError> {
        let events = self.wm.destroy_window(window_id)?;
        if let Some(surface) = self
            .topology
            .surfaces
            .iter_mut()
            .find(|surface| surface.window_id.as_ref() == Some(window_id))
        {
            surface.mapped = false;
        }
        self.synchronize_topology_focus()?;
        Ok(self.domain_update(events, true))
    }

    pub fn toggle_focused_floating(&mut self) -> Result<DomainUpdate, DomainSessionError> {
        let event = self.wm.toggle_focused_floating()?;
        Ok(self.domain_update(vec![event], true))
    }

    pub fn toggle_focused_fullscreen(&mut self) -> Result<DomainUpdate, DomainSessionError> {
        let event = self.wm.toggle_focused_fullscreen()?;
        Ok(self.domain_update(vec![event], true))
    }

    pub fn focus_direction_with_order(
        &mut self,
        order: &[WindowId],
        direction: FocusDirection,
    ) -> Result<DomainUpdate, DomainSessionError> {
        if order.is_empty() {
            return Err(WmStateError::NoFocusedWindow.into());
        }

        let focused = self.wm.focused_window_id()?.clone();
        let current_index = order
            .iter()
            .position(|window_id| window_id == &focused)
            .unwrap_or(0);
        let next_index = match direction {
            FocusDirection::Left | FocusDirection::Up => {
                (current_index + order.len() - 1) % order.len()
            }
            FocusDirection::Right | FocusDirection::Down => (current_index + 1) % order.len(),
        };

        self.focus_window(&order[next_index])
    }

    fn domain_update(&self, events: Vec<CompositorEvent>, recomputed_layout: bool) -> DomainUpdate {
        DomainUpdate {
            events,
            recomputed_layout,
            topology: self.topology.clone(),
        }
    }

    fn synchronize_topology_focus(&mut self) -> Result<(), TopologyError> {
        if self.topology.seat("seat-0").is_none() {
            self.topology.register_seat("seat-0");
        }

        self.topology.focus_seat_window(
            "seat-0",
            self.state().focused_window_id.clone(),
            self.state().current_output_id.clone(),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
    use spiders_shared::wm::{
        LayoutRef, OutputSnapshot, OutputTransform, ShellKind, StateSnapshot, WindowSnapshot,
        WorkspaceSnapshot,
    };

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 800,
                logical_height: 600,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
            }],
            workspaces: vec![WorkspaceSnapshot {
                id: WorkspaceId::from("ws-1"),
                name: "1".into(),
                output_id: Some(OutputId::from("out-1")),
                active_tags: vec!["1".into()],
                focused: true,
                visible: true,
                effective_layout: Some(LayoutRef {
                    name: "master-stack".into(),
                }),
            }],
            windows: vec![WindowSnapshot {
                id: WindowId::from("w1"),
                shell: ShellKind::XdgToplevel,
                app_id: Some("firefox".into()),
                title: Some("Firefox".into()),
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
            visible_window_ids: vec![WindowId::from("w1")],
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn domain_session_maps_window_and_creates_surface() {
        let mut session = DomainSession::new(
            WmState::from_snapshot(state().clone()),
            CompositorTopologyState::from_snapshot(&state()),
        );

        let update = session
            .map_window(WindowSnapshot {
                id: WindowId::from("w2"),
                shell: ShellKind::XdgToplevel,
                app_id: Some("alacritty".into()),
                title: Some("Terminal".into()),
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: false,
                floating: false,
                floating_rect: None,
                fullscreen: false,
                focused: false,
                urgent: false,
                output_id: Some(OutputId::from("out-1")),
                workspace_id: Some(WorkspaceId::from("ws-1")),
                tags: vec!["1".into()],
            })
            .unwrap();

        assert!(update.recomputed_layout);
        assert!(session.topology().surface("window-w2").is_some());
    }

    #[test]
    fn domain_session_focuses_window_and_syncs_seat_focus() {
        let mut snapshot = state();
        snapshot.windows.push(WindowSnapshot {
            id: WindowId::from("w2"),
            shell: ShellKind::XdgToplevel,
            app_id: Some("alacritty".into()),
            title: Some("Terminal".into()),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: true,
            floating: false,
            floating_rect: None,
            fullscreen: false,
            focused: false,
            urgent: false,
            output_id: Some(OutputId::from("out-1")),
            workspace_id: Some(WorkspaceId::from("ws-1")),
            tags: vec!["1".into()],
        });
        let mut session = DomainSession::new(
            WmState::from_snapshot(snapshot.clone()),
            CompositorTopologyState::from_snapshot(&snapshot),
        );

        let update = session.focus_window(&WindowId::from("w2")).unwrap();

        assert!(!update.recomputed_layout);
        assert_eq!(
            session.topology().seat("seat-0").unwrap().focused_window_id,
            Some(WindowId::from("w2"))
        );
    }

    #[test]
    fn domain_session_destroy_marks_surface_unmapped() {
        let mut session = DomainSession::new(
            WmState::from_snapshot(state().clone()),
            CompositorTopologyState::from_snapshot(&state()),
        );
        session
            .register_window_surface(
                "window-w1",
                WindowId::from("w1"),
                Some(OutputId::from("out-1")),
            )
            .unwrap();

        let update = session.destroy_window(&WindowId::from("w1")).unwrap();

        assert!(update.recomputed_layout);
        assert!(!session.topology().surface("window-w1").unwrap().mapped);
    }
}
