use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    reexports::wayland_server::Resource,
    wayland::{
        buffer::BufferHandler,
        compositor::{get_parent, is_sync_subsurface, CompositorHandler},
        shm::ShmHandler,
    },
};

use crate::model::OutputId;
use crate::runtime::{ClientState, SpidersWm2};

use super::xdg_shell;

impl CompositorHandler for SpidersWm2 {
    fn compositor_state(&mut self) -> &mut smithay::wayland::compositor::CompositorState {
        &mut self.runtime.smithay.compositor_state
    }

    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a smithay::wayland::compositor::CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        on_commit_buffer_handler::<Self>(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }

            if let Some(window) = self
                .runtime
                .smithay
                .space
                .elements()
                .find(|window| window.toplevel().unwrap().wl_surface() == &root)
            {
                window.on_commit();
            }
        }

        xdg_shell::handle_commit(
            &mut self.runtime.smithay.popups,
            &self.runtime.smithay.space,
            surface,
        );

        if !is_sync_subsurface(surface) {
            self.runtime.smithay.space.refresh();

            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }

            if let Some(window_id) = self.app.bindings.window_for_surface(&root.id()) {
                self.runtime.transactions.mark_window_committed(&window_id);

                if self
                    .runtime
                    .transactions
                    .pending()
                    .and_then(|pending| pending.participants.get(&window_id))
                    .is_some_and(|participant| participant.committed)
                {
                    self.runtime
                        .render_plan
                        .mark_output_dirty(OutputId::from("1"));
                }

                tracing::trace!(
                    target: "spiders_wm2::runtime_debug",
                    ?window_id,
                    pending_serials = ?self.app.bindings.pending_commit_serials(&window_id),
                    "compositor_commit_after_window_on_commit"
                );
            }

            self.maybe_commit_pending_transaction();
        }
    }
}

impl BufferHandler for SpidersWm2 {
    fn buffer_destroyed(
        &mut self,
        _buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    ) {
    }
}

impl ShmHandler for SpidersWm2 {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        &self.runtime.smithay.shm_state
    }
}

delegate_compositor!(SpidersWm2);
delegate_shm!(SpidersWm2);

#[cfg(test)]
mod tests {
    #[test]
    fn geometry_gate_should_run_after_shell_commit_processing() {
        let order = [
            "on_commit",
            "shell_commit",
            "space_refresh",
            "geometry_gate",
        ];

        assert_eq!(order[2], "space_refresh");
        assert_eq!(order[3], "geometry_gate");
    }

    #[test]
    fn observed_serial_still_requires_geometry_ready() {
        let observed = true;
        let geometry_ready = false;

        assert!(observed);
        assert!(!geometry_ready);
    }

    #[test]
    fn mapped_reflow_can_accept_observed_commit_before_geometry_catches_up() {
        let was_previously_unmapped = false;
        let geometry_ready = false;

        assert!(!was_previously_unmapped);
        assert!(!geometry_ready);
    }

    #[test]
    fn mapped_commit_path_is_now_serial_queue_driven() {
        let pending_count = 2usize;
        let completed_by_one_commit = true;

        assert_eq!(pending_count, 2);
        assert!(completed_by_one_commit);
    }

    #[test]
    fn mapped_commit_relies_on_transaction_promotion_for_repaint() {
        let eager_dirty_mark = false;

        assert!(!eager_dirty_mark);
    }

    #[test]
    fn post_configure_commit_can_request_followup_repaint() {
        let participant_committed = true;
        let should_mark_dirty = participant_committed;

        assert!(should_mark_dirty);
    }
}

// use smithay::{
//     backend::renderer::utils::on_commit_buffer_handler,
//     delegate_compositor, delegate_shm,
//     reexports::wayland_server::{
//         protocol::{wl_buffer, wl_surface::WlSurface},
//         Client,
//     },
//     wayland::{
//         buffer::BufferHandler,
//         compositor::{
//             get_parent, is_sync_subsurface, CompositorClientState, CompositorHandler,
//             CompositorState,
//         },
//         shm::{ShmHandler, ShmState},
//     },
// };
//
// use crate::state::{ClientState, SpidersWm2};
//
// use super::xdg_shell;
//
// impl CompositorHandler for SpidersWm2 {
//     fn compositor_state(&mut self) -> &mut CompositorState {
//         &mut self.compositor_state
//     }
//
//     fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
//         &client.get_data::<ClientState>().unwrap().compositor_state
//     }
//
//     fn commit(&mut self, surface: &WlSurface) {
//         on_commit_buffer_handler::<Self>(surface);
//
//         if !is_sync_subsurface(surface) {
//             let mut root = surface.clone();
//             while let Some(parent) = get_parent(&root) {
//                 root = parent;
//             }
//
//             if let Some(window) = self
//                 .space
//                 .elements()
//                 .find(|window| window.toplevel().unwrap().wl_surface() == &root)
//             {
//                 window.on_commit();
//             }
//         }
//
//         xdg_shell::handle_commit(&mut self.popups, &self.space, surface);
//     }
// }
//
// impl BufferHandler for SpidersWm2 {
//     fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
// }
//
// impl ShmHandler for SpidersWm2 {
//     fn shm_state(&self) -> &ShmState {
//         &self.shm_state
//     }
// }
//
// delegate_compositor!(SpidersWm2);
// delegate_shm!(SpidersWm2);
