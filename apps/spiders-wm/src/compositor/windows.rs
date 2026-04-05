use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use spiders_core::command::WmCommand;
use spiders_core::LayoutRect;
use spiders_scene::LayoutSnapshotNode;
use spiders_titlebar_core::{TitlebarButtonAction, titlebar_button_action_from_data};
use spiders_titlebar_native::render_titlebar_snapshot;
use tracing::{debug, info};

use crate::actions::focus::FocusUpdate;
use crate::frame_sync;
use spiders_core::window_id;
use crate::state::{ManagedWindow, NativeTitlebarHitRegion, NativeTitlebarOverlay, SpidersWm};

impl SpidersWm {
    pub(crate) fn refresh_titlebar_overlays(&mut self) {
        let Some(root) = self.titlebar_layout.snapshot_root.as_ref() else {
            self.titlebar_overlays.clear();
            return;
        };

        let mut overlays = std::collections::BTreeMap::new();
        for record in self.managed_windows() {
            let Some(window_node) = root.find_by_window_id(&record.id) else {
                continue;
            };
            let Some(titlebar) = titlebar_snapshot_node(window_node) else {
                continue;
            };
            let Some(rasterized) =
                render_titlebar_snapshot(titlebar, 1.0, self.config.options.titlebar_font.as_ref())
            else {
                continue;
            };
            overlays.insert(
                record.id.clone(),
                NativeTitlebarOverlay {
                    rect: titlebar.rect(),
                    pixels: rasterized.pixels,
                    hit_regions: titlebar_hit_regions(titlebar),
                },
            );
        }

        self.titlebar_overlays = overlays;
    }

    pub(crate) fn refresh_titlebar_snapshot_and_overlays(&mut self) {
        let visible_window_ids = self.visible_managed_window_ids();
        let tiled_window_ids = self.tiled_visible_window_ids(&visible_window_ids);
        let fullscreen_window_id =
            self.model.fullscreen_window_on_current_workspace(visible_window_ids.iter().cloned());

        if visible_window_ids.is_empty() || fullscreen_window_id.is_some() || tiled_window_ids.is_empty() {
            self.titlebar_layout.snapshot_root = None;
            self.titlebar_overlays.clear();
            return;
        }

        self.scene.clear_cache();
        self.titlebar_layout.snapshot_root =
            self.scene.compute_layout_snapshot(&self.config, &mut self.model, &tiled_window_ids);
        self.refresh_titlebar_overlays();
    }

    pub(crate) fn titlebar_action_at(
        &self,
        location: smithay::utils::Point<f64, smithay::utils::Logical>,
    ) -> Option<(spiders_core::WindowId, WmCommand)> {
        self.titlebar_overlays.iter().find_map(|(window_id, overlay)| {
            rect_contains_point(overlay.rect, location).then(|| {
                overlay.hit_regions.iter().find_map(|region| {
                    rect_contains_point(region.rect, location).then(|| (window_id.clone(), region.command.clone()))
                })
            })
            .flatten()
        })
    }

    pub fn close_focused_window(&mut self) {
        let closing_window_id = self.runtime().request_close_focused_window_selection().closing_window_id;
        info!(closing_window = ?closing_window_id, "wm close focused window request");
        let Some(focused_surface) =
            closing_window_id.and_then(|window_id| self.surface_for_window_id(window_id))
        else {
            return;
        };

        self.capture_close_snapshot(&focused_surface);

        if let Some(record) = self.managed_window_for_surface(&focused_surface) {
            if let Some(toplevel) = record.window.toplevel() {
                toplevel.send_close();
            }
        }
    }

    pub fn toggle_focused_window_floating(&mut self) {
        let (toggled_window_id, events) = {
            let mut runtime = self.runtime();
            let toggled_window_id = runtime.toggle_focused_window_floating();
            (toggled_window_id, runtime.take_events())
        };
        if toggled_window_id.is_none() {
            return;
        }

        if let Some(window_id) = toggled_window_id {
            if self.window_is_floating(&window_id)
                && let Some(output_geometry) = self.current_output_geometry()
            {
                let _ = self.ensure_floating_window_geometry(&window_id, output_geometry);
            }

            self.schedule_relayout();
            self.broadcast_runtime_events(events);
        }
    }

    pub fn toggle_focused_window_fullscreen(&mut self) {
        let (toggled_window_id, events) = {
            let mut runtime = self.runtime();
            let toggled_window_id = runtime.toggle_focused_window_fullscreen();
            (toggled_window_id, runtime.take_events())
        };
        if toggled_window_id.is_none() {
            return;
        }

        self.schedule_relayout();
        self.broadcast_runtime_events(events);
    }

    pub fn add_window(&mut self, window: Window) {
        let window_id = window_id(self.next_window_id);
        self.next_window_id += 1;
        let events = {
            let mut runtime = self.runtime();
            let _ = runtime.place_new_window(window_id.clone());
            runtime.take_events()
        };

        self.insert_managed_window(window_id.clone(), window);
        self.broadcast_runtime_events(events);

        if let Some(toplevel) = self
            .managed_window_for_id(&window_id)
            .and_then(|record| record.window.toplevel().cloned())
        {
            crate::frame_sync::install_window_pre_commit_hook(&toplevel);
        }

        info!(window = %window_id.0, total_windows = self.managed_window_count(), "wm added window");
        self.log_managed_window_state("after add window");
    }

    pub fn handle_window_unmap(&mut self, surface: &WlSurface) {
        self.capture_close_snapshot(surface);

        let window_update = if let Some(record) = self.find_window_mut(surface) {
            if !record.mapped {
                None
            } else {
                record.mapped = false;
                let unmap = record.frame_sync.begin_unmap();
                Some((record.id.clone(), record.window.clone(), unmap))
            }
        } else {
            None
        };

        let Some((window_id, window, unmap)) = window_update else {
            return;
        };
        self.titlebar_overlays.remove(&window_id);
        let snapshot = unmap.snapshot;
        let location = self
            .element_location(&window)
            .map(|location| location - window.geometry().loc);
        let window_order = self.managed_window_ids();

        let (focus_update, events) = {
            let mut runtime = self.runtime();
            let focus_update = runtime.unmap_window(window_id.clone(), window_order);
            (focus_update, runtime.take_events())
        };
        let closing = self.window_is_closing(&window_id);

        info!(
            window = %window_id.0,
            closing,
            focus_update = ?focus_update,
            "wm close start"
        );

        self.unmap_window_element(&window);

        self.log_managed_window_state("after close start");

        self.broadcast_runtime_events(events);
        self.apply_focus_update(focus_update);
        let relayout_transaction = self.closing_overlay_transaction();

        if let Some(result) =
            self.frame_sync
                .push_closing_overlay(snapshot, location, relayout_transaction)
        {
            debug!(
                window = %window_id.0,
                location = ?location,
                geometry_loc = ?window.geometry().loc,
                carried_overlays = result.carried_overlays,
                transaction = result.transaction_debug_id,
                "wm added closing snapshot overlay"
            );
        }
    }

    pub fn finalize_window_destroy(&mut self, surface: &WlSurface) {
        self.capture_close_snapshot(surface);

        let destroy_overlay = if let Some(record) = self.find_window_mut(surface) {
            if record.mapped {
                let unmap = record.frame_sync.begin_unmap();
                let snapshot = unmap.snapshot;
                Some((record.window.clone(), snapshot))
            } else {
                None
            }
        } else {
            None
        };

        let Some(position) = self.managed_window_position_for_surface(surface) else {
            return;
        };

        let destroy_overlay = destroy_overlay.map(|(window, snapshot)| {
            let location = self
                .element_location(&window)
                .map(|location| location - window.geometry().loc);
            (window, location, snapshot)
        });

        let window_order = self.managed_window_ids();
        let record = self.remove_managed_window_at(position);
        let window_id = record.id.clone();
        self.titlebar_overlays.remove(&window_id);
        let mut focus_update = FocusUpdate::Unchanged;
        let mut runtime_events = Vec::new();

        if record.mapped {
            self.unmap_window_element(&record.window);
            let mut runtime = self.runtime();
            focus_update = runtime.unmap_window(window_id.clone(), window_order.clone());
            runtime_events.extend(runtime.take_events());
        }

        let remove_update = {
            let mut runtime = self.runtime();
            let remove_update = runtime.remove_window(window_id.clone(), window_order);
            runtime_events.extend(runtime.take_events());
            remove_update
        };

        if matches!(focus_update, FocusUpdate::Unchanged) {
            focus_update = remove_update;
        }

        info!(
            window = %window_id.0,
            was_mapped = record.mapped,
            focus_update = ?focus_update,
            "wm finalized window destroy"
        );

        self.log_managed_window_state("after window destroy");

        self.broadcast_runtime_events(runtime_events);
        self.apply_focus_update(focus_update);

        if let Some((window, location, snapshot)) = destroy_overlay {
            let relayout_transaction = self.closing_overlay_transaction();

            if let Some(result) =
                self.frame_sync
                    .push_closing_overlay(snapshot, location, relayout_transaction)
            {
                debug!(
                    window = %window_id.0,
                    location = ?location,
                    geometry_loc = ?window.geometry().loc,
                    carried_overlays = result.carried_overlays,
                    transaction = result.transaction_debug_id,
                    "wm added closing snapshot overlay during destroy"
                );
            }
        } else {
            self.schedule_relayout();
        }
    }

    pub(crate) fn capture_close_snapshot(&mut self, surface: &WlSurface) {
        let output = self.current_output_cloned();
        let Some(output) = output else {
            return;
        };

        let scale = output.current_scale().fractional_scale().into();
        let snapshot = match self.backend.as_mut() {
            Some(backend) => {
                frame_sync::capture_close_snapshot(backend.renderer(), surface, scale, 1.0)
            }
            None => None,
        };

        if let Some(snapshot) = snapshot
            && let Some(record) = self.find_window_mut(surface)
        {
            record.frame_sync.store_close_snapshot(snapshot);
            debug!(window = %record.id.0, "wm captured close snapshot");
        }
    }

    pub fn find_window_mut(&mut self, surface: &WlSurface) -> Option<&mut ManagedWindow> {
        self.managed_window_mut_for_surface(surface)
    }

    pub fn is_known_window_mapped(&self, surface: &WlSurface) -> bool {
        self.managed_window_for_surface(surface)
            .is_some_and(|record| record.mapped)
    }

    pub fn handle_window_commit(&mut self, surface: &WlSurface) {
        let window_update = if let Some(record) = self.find_window_mut(surface) {
            let first_map = !record.mapped;
            if first_map {
                record.mapped = true;
            }

            let ready_layout = record.frame_sync.take_ready_layout();

            debug!(
                window = %record.id.0,
                mapped = record.mapped,
                first_map,
                ready_layout = ?ready_layout,
                pending_configures = record.frame_sync.has_pending_configures(),
                "wm handle window commit"
            );
            Some((
                record.id.clone(),
                record.window.clone(),
                first_map,
                ready_layout,
            ))
        } else {
            None
        };

        if let Some((window_id, window, first_map, ready_layout)) = window_update {
            window.on_commit();

            let layout = ready_layout.or_else(|| {
                first_map
                    .then(|| self.planned_layout_for_surface(surface))
                    .flatten()
            });

            if let Some((location, size)) = layout {
                self.map_window_element(window.clone(), location);
                debug!(
                    window = %window_id.0,
                    location = ?location,
                    size = ?size,
                    "wm mapped window after commit"
                );
            }

            if first_map {
                info!(window = %window_id.0, "wm first map commit");
                let events = {
                    let mut runtime = self.runtime();
                    let _ = runtime.sync_window_mapped(window_id, true);
                    runtime.take_events()
                };
                self.broadcast_runtime_events(events);
                self.schedule_relayout();
                self.set_focus_with_new_serial(Some(surface.clone()));
            }
        }
    }

    fn live_frame_sync_transaction(&self) -> Option<frame_sync::SyncHandle> {
        self.managed_windows()
            .iter()
            .find_map(|record| record.frame_sync.live_transaction())
    }

    fn closing_overlay_transaction(&mut self) -> Option<frame_sync::SyncHandle> {
        self.start_relayout()
            .or_else(|| self.live_frame_sync_transaction())
            .or_else(|| Some(frame_sync::new_sync_handle(&self.event_loop)))
    }
}

fn titlebar_snapshot_node(node: &LayoutSnapshotNode) -> Option<&LayoutSnapshotNode> {
    node.children().iter().find(|child| {
        matches!(child, LayoutSnapshotNode::Content { meta, .. } if meta.name.as_deref() == Some("titlebar"))
    })
}

fn titlebar_hit_regions(node: &LayoutSnapshotNode) -> Vec<NativeTitlebarHitRegion> {
    let mut regions = Vec::new();
    collect_titlebar_hit_regions(node, &mut regions);
    regions
}

fn collect_titlebar_hit_regions(node: &LayoutSnapshotNode, out: &mut Vec<NativeTitlebarHitRegion>) {
    if let LayoutSnapshotNode::Content { meta, rect, .. } = node
        && let Some(command) = titlebar_action_command(titlebar_button_action_from_data(&meta.data))
    {
        out.push(NativeTitlebarHitRegion { rect: *rect, command });
    }

    for child in node.children() {
        collect_titlebar_hit_regions(child, out);
    }
}

fn titlebar_action_command(action: Option<TitlebarButtonAction>) -> Option<WmCommand> {
    match action {
        Some(TitlebarButtonAction::Close) => Some(WmCommand::CloseFocusedWindow),
        Some(TitlebarButtonAction::ToggleFullscreen) => Some(WmCommand::ToggleFullscreen),
        Some(TitlebarButtonAction::ToggleFloating) => Some(WmCommand::ToggleFloating),
        _ => None,
    }
}

fn rect_contains_point(
    rect: LayoutRect,
    point: smithay::utils::Point<f64, smithay::utils::Logical>,
) -> bool {
    point.x >= f64::from(rect.x)
        && point.x < f64::from(rect.x + rect.width)
        && point.y >= f64::from(rect.y)
        && point.y < f64::from(rect.y + rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::LayoutNodeMeta;

    #[test]
    fn titlebar_hit_regions_collect_button_commands_from_metadata() {
        let mut button_meta = LayoutNodeMeta::default();
        button_meta.name = Some("titlebar-button".into());
        button_meta.data.insert(
            spiders_titlebar_core::TITLEBAR_ACTION_KEY.to_string(),
            "close".into(),
        );

        let titlebar = LayoutSnapshotNode::Content {
            meta: LayoutNodeMeta {
                name: Some("titlebar".into()),
                ..LayoutNodeMeta::default()
            },
            rect: LayoutRect { x: 0.0, y: 0.0, width: 200.0, height: 28.0 },
            styles: None,
            text: None,
            children: vec![LayoutSnapshotNode::Content {
                meta: button_meta,
                rect: LayoutRect { x: 8.0, y: 5.0, width: 18.0, height: 18.0 },
                styles: None,
                text: None,
                children: Vec::new(),
            }],
        };

        let hit_regions = titlebar_hit_regions(&titlebar);

        assert_eq!(hit_regions.len(), 1);
        assert_eq!(hit_regions[0].rect, LayoutRect { x: 8.0, y: 5.0, width: 18.0, height: 18.0 });
        assert_eq!(hit_regions[0].command, WmCommand::CloseFocusedWindow);
    }

    #[test]
    fn titlebar_snapshot_node_finds_injected_titlebar_child() {
        let window = LayoutSnapshotNode::Window {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 },
            styles: None,
            window_id: None,
            children: vec![LayoutSnapshotNode::Content {
                meta: LayoutNodeMeta {
                    name: Some("titlebar".into()),
                    ..LayoutNodeMeta::default()
                },
                rect: LayoutRect { x: 0.0, y: 0.0, width: 800.0, height: 28.0 },
                styles: None,
                text: None,
                children: Vec::new(),
            }],
        };

        let node = titlebar_snapshot_node(&window).expect("titlebar child should be found");

        assert_eq!(node.meta().name.as_deref(), Some("titlebar"));
    }

    #[test]
    fn rect_contains_point_uses_half_open_bounds() {
        let rect = LayoutRect { x: 10.0, y: 20.0, width: 30.0, height: 40.0 };

        assert!(rect_contains_point(rect, (10.0, 20.0).into()));
        assert!(rect_contains_point(rect, (39.999, 59.999).into()));
        assert!(!rect_contains_point(rect, (40.0, 60.0).into()));
    }
}
