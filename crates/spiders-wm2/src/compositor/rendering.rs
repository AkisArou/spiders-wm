use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::output::Output;
use smithay::utils::Rectangle;
use smithay::wayland::compositor::CompositorHandler;

use crate::frame_sync::SnapshotRenderElement;
use crate::state::SpidersWm;

impl SpidersWm {
    pub(crate) fn notify_blocker_cleared(&mut self) {
        let display_handle = self.display_handle.clone();
        while let Ok(client) = self.blocker_cleared_rx.try_recv() {
            self.client_compositor_state(&client)
                .blocker_cleared(self, &display_handle);
        }
    }

    pub(crate) fn prune_completed_closing_overlays(&mut self) {
        self.frame_sync.prune_completed_closing_overlays();
    }

    pub(crate) fn send_frames_for_windows(&self, output: &Output) {
        for record in &self.managed_windows {
            if !record.mapped {
                continue;
            }

            record.window.send_frame(
                output,
                self.start_time.elapsed(),
                Some(std::time::Duration::ZERO),
                |_, _| Some(output.clone()),
            );
        }
    }

    pub(crate) fn render_output_frame(
        &mut self,
        output: &Output,
        damage_tracker: &mut OutputDamageTracker,
    ) {
        self.notify_blocker_cleared();
        self.prune_completed_closing_overlays();

        let mut backend = self
            .backend
            .take()
            .expect("winit backend missing during redraw");
        let size = backend.window_size();
        let damage = Rectangle::from_size(size);

        {
            let (renderer, mut framebuffer) = backend.bind().expect("failed to bind winit backend");
            let scale = output.current_scale().fractional_scale().into();
            let custom_elements = self.frame_sync.render_elements(renderer, scale, 1.0);

            smithay::desktop::space::render_output::<_, SnapshotRenderElement, _, _>(
                output,
                renderer,
                &mut framebuffer,
                1.0,
                0,
                [&self.space],
                &custom_elements,
                damage_tracker,
                [0.08, 0.08, 0.1, 1.0],
            )
            .expect("failed to render output");
        }

        backend
            .submit(Some(&[damage]))
            .expect("failed to submit frame");

        self.send_frames_for_windows(output);

        self.space.refresh();
        self.popups.cleanup();
        let _ = self.display_handle.flush_clients();
        backend.window().request_redraw();
        self.backend = Some(backend);
    }
}
