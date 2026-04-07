use smithay::desktop::Window;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use tracing::{debug, info};

use crate::actions::focus::FocusUpdate;
use crate::frame_sync;
use crate::state::{ManagedWindow, SpidersWm};
use spiders_core::window_id;

impl SpidersWm {
    pub fn close_focused_window(&mut self) {
        let closing_window_id =
            self.runtime().request_close_focused_window_selection().closing_window_id;
        info!(closing_window = ?closing_window_id, "wm close focused window request");
        let Some(focused_surface) =
            closing_window_id.and_then(|window_id| self.surface_for_window_id(window_id))
        else {
            return;
        };

        self.capture_close_snapshot(&focused_surface);

        if let Some(record) = self.managed_window_for_surface(&focused_surface)
            && let Some(toplevel) = record.window.toplevel()
        {
            toplevel.send_close();
            if let Err(error) = self.display_handle.flush_clients() {
                debug!(?error, "failed to flush Wayland clients after close request");
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
        let debug_window_id = window_id.to_string();
        let snapshot = unmap.snapshot;
        let location =
            self.element_location(&window).map(|location| location - window.geometry().loc);
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
        self.debug_render_event("window-unmap", Some(&debug_window_id), || {
            format!(
                "closing={closing} location={location:?} has_snapshot={} focus_update={focus_update:?}",
                snapshot.is_some()
            )
        });

        self.unmap_window_element(&window);

        self.log_managed_window_state("after close start");

        self.broadcast_runtime_events(events);
        self.apply_focus_update(focus_update);
        let relayout_transaction = self.closing_overlay_transaction();

        if let Some(result) =
            self.frame_sync.push_closing_overlay(snapshot, location, relayout_transaction)
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
            let location =
                self.element_location(&window).map(|location| location - window.geometry().loc);
            (window, location, snapshot)
        });

        let window_order = self.managed_window_ids();
        let record = self.remove_managed_window_at(position);
        let window_id = record.id.clone();
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
        let debug_window_id = window_id.to_string();
        self.debug_render_event("window-destroy", Some(&debug_window_id), || {
            format!("was_mapped={} focus_update={focus_update:?}", record.mapped)
        });

        self.log_managed_window_state("after window destroy");

        self.broadcast_runtime_events(runtime_events);
        self.apply_focus_update(focus_update);

        if let Some((window, location, snapshot)) = destroy_overlay {
            let relayout_transaction = self.closing_overlay_transaction();

            if let Some(result) =
                self.frame_sync.push_closing_overlay(snapshot, location, relayout_transaction)
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
        self.managed_window_for_surface(surface).is_some_and(|record| record.mapped)
    }

    pub fn handle_window_commit(&mut self, surface: &WlSurface) {
        let window_update = if let Some(record) = self.find_window_mut(surface) {
            let first_map = !record.mapped;
            if first_map {
                record.mapped = true;
            }

            let ready_layout = record.frame_sync.take_ready_layout();
            let relayout_needed_after_configure =
                record.frame_sync.take_relayout_needed_after_configure();
            let render_debug_details = format!(
                "mapped={} first_map={} ready_layout={ready_layout:?} pending_configures={} has_close_snapshot={} relayout_needed_after_configure={}",
                record.mapped,
                first_map,
                record.frame_sync.has_pending_configures(),
                record.frame_sync.has_close_snapshot(),
                relayout_needed_after_configure,
            );

            debug!(
                window = %record.id.0,
                mapped = record.mapped,
                first_map,
                ready_layout = ?ready_layout,
                pending_configures = record.frame_sync.has_pending_configures(),
                relayout_needed_after_configure,
                "wm handle window commit"
            );
            Some((
                record.id.clone(),
                record.window.clone(),
                first_map,
                ready_layout,
                relayout_needed_after_configure,
                render_debug_details,
            ))
        } else {
            None
        };

        if let Some((
            window_id,
            window,
            first_map,
            ready_layout,
            relayout_needed_after_configure,
            render_debug_details,
        )) = window_update
        {
            let debug_window_id = window_id.to_string();
            self.debug_render_event("window-commit", Some(&debug_window_id), || {
                render_debug_details
            });
            window.on_commit();

            if first_map {
                let events = {
                    let mut runtime = self.runtime();
                    let _ = runtime.sync_window_mapped(window_id.clone(), true);
                    let _ =
                        runtime.request_focus_window_selection("winit", Some(window_id.clone()));
                    runtime.take_events()
                };
                self.broadcast_runtime_events(events);
            }

            let layout = ready_layout
                .or_else(|| first_map.then(|| self.planned_layout_for_surface(surface)).flatten());

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
                self.apply_modeled_focus(
                    Some(window_id),
                    smithay::utils::SERIAL_COUNTER.next_serial(),
                );
                debug!(
                    window = %debug_window_id,
                    relayout_already_queued = self.relayout_queued,
                    "wm first-map relayout queued"
                );
                self.queue_first_map_burst_relayout();
            } else if relayout_needed_after_configure {
                debug!(window = %debug_window_id, "wm queued relayout after pending configure commit");
                self.queue_relayout();
            }
        }
    }

    fn live_frame_sync_transaction(&self) -> Option<frame_sync::SyncHandle> {
        self.managed_windows().iter().find_map(|record| record.frame_sync.live_transaction())
    }

    fn closing_overlay_transaction(&mut self) -> Option<frame_sync::SyncHandle> {
        self.start_relayout()
            .or_else(|| self.live_frame_sync_transaction())
            .or_else(|| Some(frame_sync::new_sync_handle(&self.event_loop)))
    }
}
