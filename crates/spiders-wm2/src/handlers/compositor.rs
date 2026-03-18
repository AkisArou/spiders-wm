use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    delegate_compositor, delegate_shm,
    wayland::{
        buffer::BufferHandler,
        compositor::{CompositorHandler, get_parent, is_sync_subsurface},
        shm::ShmHandler,
    },
};

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
