use spiders_shared::ids::{OutputId, WindowId};
use spiders_shared::wm::{OutputSnapshot, StateSnapshot};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TopologyError {
    #[error("output not found: {0}")]
    OutputNotFound(OutputId),
    #[error("seat not found: {0}")]
    SeatNotFound(String),
    #[error("surface not found: {0}")]
    SurfaceNotFound(String),
    #[error("window surface not found for window: {0}")]
    WindowSurfaceNotFound(WindowId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceRole {
    Window,
    Popup,
    Layer,
    Lock,
    Unmanaged,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerSurfaceTier {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerKeyboardInteractivity {
    None,
    Exclusive,
    OnDemand,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerExclusiveZone {
    Neutral,
    Exclusive(u32),
    DontCare,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LayerSurfaceMetadata {
    pub namespace: String,
    pub tier: LayerSurfaceTier,
    pub keyboard_interactivity: LayerKeyboardInteractivity,
    pub exclusive_zone: LayerExclusiveZone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceState {
    pub id: String,
    pub role: SurfaceRole,
    pub output_id: Option<OutputId>,
    pub window_id: Option<WindowId>,
    pub parent_surface_id: Option<String>,
    pub layer_metadata: Option<LayerSurfaceMetadata>,
    pub mapped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatState {
    pub name: String,
    pub focused_output_id: Option<OutputId>,
    pub focused_window_id: Option<WindowId>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputState {
    pub snapshot: OutputSnapshot,
    pub mapped_surface_ids: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositorTopologyState {
    pub outputs: Vec<OutputState>,
    pub seats: Vec<SeatState>,
    pub surfaces: Vec<SurfaceState>,
    pub active_output_id: Option<OutputId>,
    pub active_seat_name: Option<String>,
}

impl CompositorTopologyState {
    pub fn from_snapshot(state: &StateSnapshot) -> Self {
        Self {
            outputs: state
                .outputs
                .iter()
                .cloned()
                .map(|snapshot| OutputState {
                    snapshot,
                    mapped_surface_ids: Vec::new(),
                    active: false,
                })
                .collect(),
            seats: Vec::new(),
            surfaces: Vec::new(),
            active_output_id: state.current_output_id.clone(),
            active_seat_name: None,
        }
    }

    pub fn output(&self, output_id: &OutputId) -> Option<&OutputState> {
        self.outputs
            .iter()
            .find(|output| &output.snapshot.id == output_id)
    }

    pub fn seat(&self, seat_name: &str) -> Option<&SeatState> {
        self.seats.iter().find(|seat| seat.name == seat_name)
    }

    pub fn active_output(&self) -> Option<&OutputState> {
        self.active_output_id
            .as_ref()
            .and_then(|output_id| self.output(output_id))
    }

    pub fn active_seat(&self) -> Option<&SeatState> {
        self.active_seat_name
            .as_ref()
            .and_then(|seat_name| self.seat(seat_name))
    }

    pub fn surface(&self, surface_id: &str) -> Option<&SurfaceState> {
        self.surfaces
            .iter()
            .find(|surface| surface.id == surface_id)
    }

    pub fn register_output(&mut self, output: OutputSnapshot) {
        if let Some(existing) = self
            .outputs
            .iter_mut()
            .find(|existing| existing.snapshot.id == output.id)
        {
            existing.snapshot = output;
            existing.active = self.active_output_id.as_ref() == Some(&existing.snapshot.id);
            return;
        }

        self.outputs.push(OutputState {
            active: self.active_output_id.as_ref() == Some(&output.id),
            snapshot: output,
            mapped_surface_ids: Vec::new(),
        });
    }

    pub fn register_seat(&mut self, seat_name: impl Into<String>) -> &SeatState {
        let seat_name = seat_name.into();

        if self.seat(&seat_name).is_none() {
            self.seats.push(SeatState {
                name: seat_name.clone(),
                focused_output_id: None,
                focused_window_id: None,
                active: self.active_seat_name.as_deref() == Some(seat_name.as_str()),
            });
        }

        self.seat(&seat_name).expect("seat was just inserted")
    }

    pub fn unregister_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        if !self
            .outputs
            .iter()
            .any(|output| output.snapshot.id == *output_id)
        {
            return Err(TopologyError::OutputNotFound(output_id.clone()));
        }

        self.outputs
            .retain(|output| output.snapshot.id != *output_id);
        for surface in &mut self.surfaces {
            if surface.output_id.as_ref() == Some(output_id) {
                surface.output_id = None;
            }
        }
        if self.active_output_id.as_ref() == Some(output_id) {
            self.active_output_id = None;
        }
        for seat in &mut self.seats {
            if seat.focused_output_id.as_ref() == Some(output_id) {
                seat.focused_output_id = None;
            }
        }
        Ok(())
    }

    pub fn unregister_seat(&mut self, seat_name: &str) -> Result<(), TopologyError> {
        if !self.seats.iter().any(|seat| seat.name == seat_name) {
            return Err(TopologyError::SeatNotFound(seat_name.to_owned()));
        }

        self.seats.retain(|seat| seat.name != seat_name);
        if self.active_seat_name.as_deref() == Some(seat_name) {
            self.active_seat_name = None;
        }
        Ok(())
    }

    pub fn unregister_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        let surface = self
            .surface(surface_id)
            .cloned()
            .ok_or_else(|| TopologyError::SurfaceNotFound(surface_id.to_owned()))?;

        let child_surface_ids = self.child_surface_ids(surface_id);
        for child_surface_id in child_surface_ids {
            self.unregister_surface(&child_surface_id)?;
        }

        if let Some(output_id) = surface.output_id.as_ref() {
            if let Some(output) = self
                .outputs
                .iter_mut()
                .find(|output| output.snapshot.id == *output_id)
            {
                output.mapped_surface_ids.retain(|id| id != surface_id);
            }
        }

        self.surfaces.retain(|entry| entry.id != surface_id);
        Ok(())
    }

    pub fn unregister_window_surface(&mut self, window_id: &WindowId) -> Result<(), TopologyError> {
        let surface_id = self
            .surfaces
            .iter()
            .find(|surface| surface.window_id.as_ref() == Some(window_id))
            .map(|surface| surface.id.clone())
            .ok_or_else(|| TopologyError::WindowSurfaceNotFound(window_id.clone()))?;
        self.unregister_surface(&surface_id)
    }

    pub fn activate_output(&mut self, output_id: &OutputId) -> Result<&OutputState, TopologyError> {
        let mut found = false;
        for output in &mut self.outputs {
            let active = output.snapshot.id == *output_id;
            output.active = active;
            if active {
                found = true;
            }
        }

        if !found {
            return Err(TopologyError::OutputNotFound(output_id.clone()));
        }

        self.active_output_id = Some(output_id.clone());
        self.output(output_id)
            .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))
    }

    pub fn disable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        let output = self
            .outputs
            .iter_mut()
            .find(|output| output.snapshot.id == *output_id)
            .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))?;
        output.snapshot.enabled = false;
        output.active = false;
        if self.active_output_id.as_ref() == Some(output_id) {
            self.active_output_id = None;
        }
        Ok(())
    }

    pub fn enable_output(&mut self, output_id: &OutputId) -> Result<(), TopologyError> {
        let output = self
            .outputs
            .iter_mut()
            .find(|output| output.snapshot.id == *output_id)
            .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))?;
        output.snapshot.enabled = true;
        Ok(())
    }

    pub fn activate_seat(&mut self, seat_name: &str) -> Result<&SeatState, TopologyError> {
        let mut found = false;
        for seat in &mut self.seats {
            let active = seat.name == seat_name;
            seat.active = active;
            if active {
                found = true;
            }
        }

        if !found {
            return Err(TopologyError::SeatNotFound(seat_name.to_owned()));
        }

        self.active_seat_name = Some(seat_name.to_owned());
        self.seat(seat_name)
            .ok_or_else(|| TopologyError::SeatNotFound(seat_name.to_owned()))
    }

    pub fn map_window_surface(
        &mut self,
        surface_id: impl Into<String>,
        window_id: WindowId,
        output_id: Option<OutputId>,
    ) -> Result<&SurfaceState, TopologyError> {
        let surface_id = surface_id.into();

        if let Some(output_id) = output_id.as_ref() {
            self.output(output_id)
                .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))?;
        }

        if let Some(existing) = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == surface_id)
        {
            existing.role = SurfaceRole::Window;
            existing.window_id = Some(window_id.clone());
            existing.output_id = output_id.clone();
            existing.mapped = true;
        } else {
            self.surfaces.push(SurfaceState {
                id: surface_id.clone(),
                role: SurfaceRole::Window,
                output_id: output_id.clone(),
                window_id: Some(window_id.clone()),
                parent_surface_id: None,
                layer_metadata: None,
                mapped: true,
            });
        }

        if let Some(output_id) = output_id {
            let output = self
                .outputs
                .iter_mut()
                .find(|output| output.snapshot.id == output_id)
                .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))?;
            if !output.mapped_surface_ids.iter().any(|id| id == &surface_id) {
                output.mapped_surface_ids.push(surface_id.clone());
            }
        }

        self.surface(&surface_id)
            .ok_or(TopologyError::SurfaceNotFound(surface_id))
    }

    pub fn register_surface(
        &mut self,
        surface_id: impl Into<String>,
        role: SurfaceRole,
        output_id: Option<OutputId>,
        parent_surface_id: Option<String>,
    ) -> Result<&SurfaceState, TopologyError> {
        self.register_surface_with_layer_metadata(
            surface_id,
            role,
            output_id,
            parent_surface_id,
            None,
        )
    }

    pub fn register_layer_surface(
        &mut self,
        surface_id: impl Into<String>,
        output_id: OutputId,
        metadata: LayerSurfaceMetadata,
    ) -> Result<&SurfaceState, TopologyError> {
        self.register_surface_with_layer_metadata(
            surface_id,
            SurfaceRole::Layer,
            Some(output_id),
            None,
            Some(metadata),
        )
    }

    fn register_surface_with_layer_metadata(
        &mut self,
        surface_id: impl Into<String>,
        role: SurfaceRole,
        output_id: Option<OutputId>,
        parent_surface_id: Option<String>,
        layer_metadata: Option<LayerSurfaceMetadata>,
    ) -> Result<&SurfaceState, TopologyError> {
        let surface_id = surface_id.into();

        if let Some(output_id) = output_id.as_ref() {
            self.output(output_id)
                .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))?;
        }

        if self.surface(&surface_id).is_some() {
            self.update_surface_attachment(&surface_id, output_id.clone())?;
            let surface = self
                .surfaces
                .iter_mut()
                .find(|surface| surface.id == surface_id)
                .expect("surface exists after attachment update");
            surface.role = role;
            surface.parent_surface_id = parent_surface_id;
            surface.layer_metadata = layer_metadata;
            surface.mapped = true;

            if let Some(output_id) = output_id.as_ref() {
                self.attach_surface_to_output(&surface_id, output_id)?;
            }
        } else {
            self.surfaces.push(SurfaceState {
                id: surface_id.clone(),
                role,
                output_id: output_id.clone(),
                window_id: None,
                parent_surface_id,
                layer_metadata,
                mapped: true,
            });
            if let Some(output_id) = output_id {
                self.attach_surface_to_output(&surface_id, &output_id)?;
            }
        }

        self.surface(&surface_id)
            .ok_or(TopologyError::SurfaceNotFound(surface_id))
    }

    pub fn unmap_surface(&mut self, surface_id: &str) -> Result<(), TopologyError> {
        let output_id = self
            .surface(surface_id)
            .ok_or_else(|| TopologyError::SurfaceNotFound(surface_id.to_owned()))?
            .output_id
            .clone();

        let child_surface_ids = self.child_surface_ids(surface_id);
        for child_surface_id in child_surface_ids {
            self.unmap_surface(&child_surface_id)?;
        }

        let surface = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == surface_id)
            .ok_or_else(|| TopologyError::SurfaceNotFound(surface_id.to_owned()))?;
        surface.mapped = false;

        if let Some(output_id) = output_id {
            if let Some(output) = self
                .outputs
                .iter_mut()
                .find(|output| output.snapshot.id == output_id)
            {
                output.mapped_surface_ids.retain(|id| id != surface_id);
            }
        }

        Ok(())
    }

    pub fn update_surface_attachment(
        &mut self,
        surface_id: &str,
        output_id: Option<OutputId>,
    ) -> Result<(), TopologyError> {
        let current_output_id = self
            .surface(surface_id)
            .ok_or_else(|| TopologyError::SurfaceNotFound(surface_id.to_owned()))?
            .output_id
            .clone();

        if current_output_id == output_id {
            return Ok(());
        }

        if let Some(current_output_id) = current_output_id {
            if let Some(output) = self
                .outputs
                .iter_mut()
                .find(|output| output.snapshot.id == current_output_id)
            {
                output.mapped_surface_ids.retain(|id| id != surface_id);
            }
        }

        if let Some(output_id) = output_id.as_ref() {
            self.attach_surface_to_output(surface_id, output_id)?;
        }

        let surface = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == surface_id)
            .ok_or_else(|| TopologyError::SurfaceNotFound(surface_id.to_owned()))?;
        surface.output_id = output_id;
        Ok(())
    }

    fn attach_surface_to_output(
        &mut self,
        surface_id: &str,
        output_id: &OutputId,
    ) -> Result<(), TopologyError> {
        let output = self
            .outputs
            .iter_mut()
            .find(|output| output.snapshot.id == *output_id)
            .ok_or_else(|| TopologyError::OutputNotFound(output_id.clone()))?;
        if !output.mapped_surface_ids.iter().any(|id| id == surface_id) {
            output.mapped_surface_ids.push(surface_id.to_owned());
        }
        Ok(())
    }

    fn child_surface_ids(&self, parent_surface_id: &str) -> Vec<String> {
        self.surfaces
            .iter()
            .filter(|surface| surface.parent_surface_id.as_deref() == Some(parent_surface_id))
            .map(|surface| surface.id.clone())
            .collect()
    }

    pub fn focus_seat_window(
        &mut self,
        seat_name: &str,
        window_id: Option<WindowId>,
        output_id: Option<OutputId>,
    ) -> Result<&SeatState, TopologyError> {
        let seat_index = self
            .seats
            .iter()
            .position(|seat| seat.name == seat_name)
            .ok_or_else(|| TopologyError::SeatNotFound(seat_name.to_owned()))?;

        let should_activate = !self.seats[seat_index].active;
        if should_activate {
            self.active_seat_name = Some(seat_name.to_owned());
            for entry in &mut self.seats {
                entry.active = entry.name == seat_name;
            }
        }

        let seat = &mut self.seats[seat_index];
        seat.focused_window_id = window_id;
        seat.focused_output_id = output_id;
        Ok(seat)
    }
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::{OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot};

    use super::*;

    fn state() -> StateSnapshot {
        StateSnapshot {
            focused_window_id: None,
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: Some(WorkspaceId::from("ws-1")),
            outputs: vec![OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 1920,
                logical_height: 1080,
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
                effective_layout: None,
            }],
            windows: vec![],
            visible_window_ids: vec![],
            tag_names: vec!["1".into()],
        }
    }

    #[test]
    fn topology_initializes_outputs_from_snapshot() {
        let topology = CompositorTopologyState::from_snapshot(&state());

        assert_eq!(topology.outputs.len(), 1);
        assert_eq!(topology.outputs[0].snapshot.id, OutputId::from("out-1"));
    }

    #[test]
    fn topology_maps_window_surfaces_per_output() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());

        topology
            .map_window_surface(
                "surface-1",
                WindowId::from("w1"),
                Some(OutputId::from("out-1")),
            )
            .unwrap();

        assert_eq!(
            topology
                .output(&OutputId::from("out-1"))
                .unwrap()
                .mapped_surface_ids,
            vec!["surface-1".to_string()]
        );
        assert_eq!(
            topology.surface("surface-1").unwrap().window_id,
            Some(WindowId::from("w1"))
        );
    }

    #[test]
    fn topology_tracks_seat_focus() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology.register_seat("seat-0");

        topology
            .focus_seat_window(
                "seat-0",
                Some(WindowId::from("w1")),
                Some(OutputId::from("out-1")),
            )
            .unwrap();

        let seat = topology.seat("seat-0").unwrap();
        assert_eq!(seat.focused_window_id, Some(WindowId::from("w1")));
        assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));
        assert!(seat.active);
    }

    #[test]
    fn topology_activates_and_disables_outputs() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        let mut topology = CompositorTopologyState::from_snapshot(&snapshot);

        topology.activate_output(&OutputId::from("out-2")).unwrap();
        topology.disable_output(&OutputId::from("out-2")).unwrap();
        topology.enable_output(&OutputId::from("out-2")).unwrap();

        assert_eq!(topology.active_output_id, None);
        assert!(
            topology
                .output(&OutputId::from("out-2"))
                .unwrap()
                .snapshot
                .enabled
        );
    }

    #[test]
    fn topology_tracks_active_seat_selection() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology.register_seat("seat-0");
        topology.register_seat("seat-1");

        topology.activate_seat("seat-1").unwrap();

        assert_eq!(topology.active_seat_name.as_deref(), Some("seat-1"));
        assert!(topology.seat("seat-1").unwrap().active);
        assert!(!topology.seat("seat-0").unwrap().active);
    }

    #[test]
    fn topology_registers_popup_and_unmanaged_surfaces() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());

        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                Some("window-w1".into()),
            )
            .unwrap();
        topology
            .register_surface("overlay-1", SurfaceRole::Unmanaged, None, None)
            .unwrap();

        assert_eq!(
            topology.surface("popup-1").unwrap().role,
            SurfaceRole::Popup
        );
        assert_eq!(
            topology
                .surface("popup-1")
                .unwrap()
                .parent_surface_id
                .as_deref(),
            Some("window-w1")
        );
        assert_eq!(
            topology.surface("overlay-1").unwrap().role,
            SurfaceRole::Unmanaged
        );
    }

    #[test]
    fn topology_updates_surface_output_attachment() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        let mut topology = CompositorTopologyState::from_snapshot(&snapshot);
        topology
            .register_surface(
                "layer-1",
                SurfaceRole::Layer,
                Some(OutputId::from("out-1")),
                None,
            )
            .unwrap();

        topology
            .update_surface_attachment("layer-1", Some(OutputId::from("out-2")))
            .unwrap();

        assert_eq!(
            topology.surface("layer-1").unwrap().output_id,
            Some(OutputId::from("out-2"))
        );
        assert!(topology
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
        assert_eq!(
            topology
                .output(&OutputId::from("out-2"))
                .unwrap()
                .mapped_surface_ids,
            vec!["layer-1".to_string()]
        );
    }

    #[test]
    fn topology_preserves_layer_metadata_across_remap() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology
            .register_layer_surface(
                "layer-1",
                OutputId::from("out-1"),
                LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(18),
                },
            )
            .unwrap();
        topology.unmap_surface("layer-1").unwrap();
        topology
            .register_layer_surface(
                "layer-1",
                OutputId::from("out-1"),
                LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(18),
                },
            )
            .unwrap();

        assert_eq!(
            topology.surface("layer-1").unwrap().layer_metadata,
            Some(LayerSurfaceMetadata {
                namespace: "panel".into(),
                tier: LayerSurfaceTier::Top,
                keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                exclusive_zone: LayerExclusiveZone::Exclusive(18),
            })
        );
    }

    #[test]
    fn topology_unregisters_output_and_clears_links() {
        let mut snapshot = state();
        snapshot.outputs.push(OutputSnapshot {
            id: OutputId::from("out-2"),
            name: "DP-1".into(),
            logical_width: 2560,
            logical_height: 1440,
            scale: 1,
            transform: OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        });
        let mut topology = CompositorTopologyState::from_snapshot(&snapshot);
        topology.register_seat("seat-0");
        topology.activate_output(&OutputId::from("out-2")).unwrap();
        topology
            .focus_seat_window(
                "seat-0",
                Some(WindowId::from("w1")),
                Some(OutputId::from("out-2")),
            )
            .unwrap();
        topology
            .register_surface(
                "layer-1",
                SurfaceRole::Layer,
                Some(OutputId::from("out-2")),
                None,
            )
            .unwrap();

        topology
            .unregister_output(&OutputId::from("out-2"))
            .unwrap();

        assert!(topology.output(&OutputId::from("out-2")).is_none());
        assert_eq!(topology.active_output_id, None);
        assert_eq!(topology.surface("layer-1").unwrap().output_id, None);
        assert_eq!(topology.seat("seat-0").unwrap().focused_output_id, None);
    }

    #[test]
    fn topology_unregisters_seats_and_surfaces() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology.register_seat("seat-0");
        topology.activate_seat("seat-0").unwrap();
        topology
            .map_window_surface(
                "window-w1",
                WindowId::from("w1"),
                Some(OutputId::from("out-1")),
            )
            .unwrap();

        topology.unregister_seat("seat-0").unwrap();
        topology
            .unregister_window_surface(&WindowId::from("w1"))
            .unwrap();

        assert!(topology.seat("seat-0").is_none());
        assert!(topology.surface("window-w1").is_none());
        assert_eq!(topology.active_seat_name, None);
    }

    #[test]
    fn topology_unregisters_generic_surfaces() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                None,
            )
            .unwrap();

        topology.unregister_surface("popup-1").unwrap();

        assert!(topology.surface("popup-1").is_none());
        assert!(topology
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
    }

    #[test]
    fn topology_unmap_removes_surface_from_output_mapped_ids() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                Some("window-w1".into()),
            )
            .unwrap();

        topology.unmap_surface("popup-1").unwrap();

        assert!(!topology.surface("popup-1").unwrap().mapped);
        assert!(topology
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
    }

    #[test]
    fn topology_remap_restores_surface_to_output_mapped_ids() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                Some("window-w1".into()),
            )
            .unwrap();
        topology.unmap_surface("popup-1").unwrap();

        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                Some("window-w1".into()),
            )
            .unwrap();

        assert!(topology.surface("popup-1").unwrap().mapped);
        assert_eq!(
            topology
                .output(&OutputId::from("out-1"))
                .unwrap()
                .mapped_surface_ids,
            vec!["popup-1".to_string()]
        );
    }

    #[test]
    fn topology_unmap_cascades_to_popup_children() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology
            .map_window_surface(
                "window-1",
                WindowId::from("w1"),
                Some(OutputId::from("out-1")),
            )
            .unwrap();
        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                Some("window-1".into()),
            )
            .unwrap();

        topology.unmap_surface("window-1").unwrap();

        assert!(!topology.surface("window-1").unwrap().mapped);
        assert!(!topology.surface("popup-1").unwrap().mapped);
        assert!(topology
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
    }

    #[test]
    fn topology_remove_cascades_to_popup_children() {
        let mut topology = CompositorTopologyState::from_snapshot(&state());
        topology
            .map_window_surface(
                "window-1",
                WindowId::from("w1"),
                Some(OutputId::from("out-1")),
            )
            .unwrap();
        topology
            .register_surface(
                "popup-1",
                SurfaceRole::Popup,
                Some(OutputId::from("out-1")),
                Some("window-1".into()),
            )
            .unwrap();

        topology.unregister_surface("window-1").unwrap();

        assert!(topology.surface("window-1").is_none());
        assert!(topology.surface("popup-1").is_none());
        assert!(topology
            .output(&OutputId::from("out-1"))
            .unwrap()
            .mapped_surface_ids
            .is_empty());
    }
}
