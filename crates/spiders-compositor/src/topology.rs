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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceRole {
    Window,
    Popup,
    Layer,
    Lock,
    Unmanaged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceState {
    pub id: String,
    pub role: SurfaceRole,
    pub output_id: Option<OutputId>,
    pub window_id: Option<WindowId>,
    pub parent_surface_id: Option<String>,
    pub mapped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatState {
    pub name: String,
    pub focused_output_id: Option<OutputId>,
    pub focused_window_id: Option<WindowId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputState {
    pub snapshot: OutputSnapshot,
    pub mapped_surface_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositorTopologyState {
    pub outputs: Vec<OutputState>,
    pub seats: Vec<SeatState>,
    pub surfaces: Vec<SurfaceState>,
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
                })
                .collect(),
            seats: Vec::new(),
            surfaces: Vec::new(),
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
            return;
        }

        self.outputs.push(OutputState {
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
            });
        }

        self.seat(&seat_name).expect("seat was just inserted")
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
            surface.mapped = true;
        } else {
            self.surfaces.push(SurfaceState {
                id: surface_id.clone(),
                role,
                output_id: output_id.clone(),
                window_id: None,
                parent_surface_id,
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
        let surface = self
            .surfaces
            .iter_mut()
            .find(|surface| surface.id == surface_id)
            .ok_or_else(|| TopologyError::SurfaceNotFound(surface_id.to_owned()))?;
        surface.mapped = false;
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

    pub fn focus_seat_window(
        &mut self,
        seat_name: &str,
        window_id: Option<WindowId>,
        output_id: Option<OutputId>,
    ) -> Result<&SeatState, TopologyError> {
        let seat = self
            .seats
            .iter_mut()
            .find(|seat| seat.name == seat_name)
            .ok_or_else(|| TopologyError::SeatNotFound(seat_name.to_owned()))?;
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
}
