use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};
use smithay::delegate_compositor;
use smithay::delegate_shm;
use smithay::reexports::wayland_server::Client;
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::{wl_buffer, wl_surface::WlSurface};
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use tracing::{debug, info};

use crate::state::SpidersWm;

use super::xdg_shell;

#[derive(Default)]
pub(crate) struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}

    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl CompositorHandler for SpidersWm {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().expect("missing client state").compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);

        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }

            let is_mapped = with_renderer_surface_state(&root, |state| state.buffer().is_some())
                .unwrap_or(false);
            let window_id = self.window_id_for_surface(&root);
            let known_mapped = self.is_known_window_mapped(&root);
            let debug_window_id = window_id.as_ref().map(ToString::to_string);

            self.debug_protocol_event("root-commit", debug_window_id.as_deref(), || {
                format!("has_buffer={is_mapped} known_mapped={known_mapped}")
            });

            debug!(
                window = ?window_id,
                has_buffer = is_mapped,
                known_mapped,
                "wm compositor root commit"
            );

            if is_mapped {
                self.handle_window_commit(&root);
            } else if known_mapped {
                info!(window = ?window_id, "wm compositor observed root unmap commit");
                self.handle_window_unmap(&root);
            }
        }

        xdg_shell::handle_commit(self, surface);
    }
}

impl BufferHandler for SpidersWm {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for SpidersWm {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}

delegate_compositor!(SpidersWm);
delegate_shm!(SpidersWm);
