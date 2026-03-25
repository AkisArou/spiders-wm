use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::output::Output;
use smithay::utils::Rectangle;

use crate::frame_sync::RenderElements;
use crate::state::SpidersWm;

impl SpidersWm {
    pub(crate) fn refresh_window_snapshots(
        &mut self,
        renderer: &mut smithay::backend::renderer::gles::GlesRenderer,
    ) {
        self.frame_sync.refresh_window_snapshots(
            renderer,
            self.managed_windows
                .iter_mut()
                .map(|record| (&record.window, record.mapped, &mut record.frame_sync)),
        );
    }

    pub(crate) fn advance_closing_windows(&mut self) {
        self.frame_sync.advance_closing_windows();
        self.flush_queued_relayout();
    }

    pub(crate) fn advance_resize_overlays(&mut self) {
        let remaps = self.frame_sync.finished_resize_overlay_mappings(
            self.managed_windows
                .iter_mut()
                .map(|record| (&record.window, &mut record.frame_sync)),
        );
        self.frame_sync.advance_resize_overlays(&mut self.space, remaps);
        self.flush_queued_relayout();
    }

    pub(crate) fn transition_render_elements(&self) -> Vec<RenderElements> {
        self.frame_sync
            .render_elements(self.managed_windows.iter().map(|record| &record.frame_sync))
    }

    pub(crate) fn send_frames_for_windows(&self, output: &Output) {
        for record in &self.managed_windows {
            if !record.frame_sync.needs_frame_callback(record.mapped) {
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
        self.advance_closing_windows();
        self.advance_resize_overlays();

        let mut backend = self
            .backend
            .take()
            .expect("winit backend missing during redraw");
        let size = backend.window_size();
        let damage = Rectangle::from_size(size);

        {
            let (renderer, mut framebuffer) =
                backend.bind().expect("failed to bind winit backend");
            self.refresh_window_snapshots(renderer);
            let transition_elements = self.transition_render_elements();
            smithay::desktop::space::render_output::<_, RenderElements, _, _>(
                output,
                renderer,
                &mut framebuffer,
                1.0,
                0,
                [&self.space],
                &transition_elements,
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