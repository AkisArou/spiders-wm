//! Window snapshots and transition overlays for frame-perfect animations.
//!
//! Snapshots capture a window's visual state at a specific point in time and are used to
//! maintain visual continuity during transitions like closes and relayouts.

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::render_elements;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::utils::draw_render_elements;
use smithay::backend::renderer::{
    Bind, Color32F, Frame, Offscreen, Renderer, element::AsRenderElements,
    gles::GlesRenderer, gles::GlesTexture,
};
use smithay::desktop::Window;
use smithay::utils::{Logical, Point, Rectangle, Scale, Size, Transform};

use super::transaction::TransactionMonitor;

render_elements! {
    pub RenderElements<=GlesRenderer>;
    Snapshot=TextureRenderElement<GlesTexture>,
}

/// A snapshot of a window's visual state at a specific point in time.
#[derive(Clone)]
pub struct WindowSnapshot {
    buffer: TextureBuffer<GlesTexture>,
    bbox: Rectangle<i32, Logical>,
}

/// Animated window during close operations.
pub struct ClosingWindow {
    pub(crate) buffer: TextureBuffer<GlesTexture>,
    pub(crate) location: Point<i32, Logical>,
    pub(crate) size: Size<i32, Logical>,
    pub(crate) monitor: TransactionMonitor,
}

/// Animated window during resize/relayout operations.
pub struct ResizingWindow {
    pub(crate) buffer: TextureBuffer<GlesTexture>,
    pub(crate) location: Point<i32, Logical>,
    pub(crate) size: Size<i32, Logical>,
    pub(crate) monitor: TransactionMonitor,
}

impl WindowSnapshot {
    /// Captures a window's current visual state (content + decorations + popups).
    pub fn capture(
        renderer: &mut GlesRenderer,
        window: &Window,
    ) -> Result<Option<Self>, smithay::backend::renderer::gles::GlesError> {
        let bbox = window.bbox_with_popups();
        if bbox.size.w <= 0 || bbox.size.h <= 0 {
            return Ok(None);
        }

        let scale = Scale::from(1.0);
        let damage = [Rectangle::from_size((bbox.size.w, bbox.size.h).into())];
        let location = Point::from((-bbox.loc.x, -bbox.loc.y))
            .to_f64()
            .to_physical_precise_round(scale);
        let elements = AsRenderElements::<GlesRenderer>::render_elements::<
            WaylandSurfaceRenderElement<GlesRenderer>,
        >(window, renderer, location, scale, 1.0);

        let mut texture = Offscreen::<GlesTexture>::create_buffer(
            renderer,
            Fourcc::Abgr8888,
            (bbox.size.w, bbox.size.h).into(),
        )?;

        {
            let mut framebuffer = renderer.bind(&mut texture)?;
            let mut frame = renderer.render(
                &mut framebuffer,
                (bbox.size.w, bbox.size.h).into(),
                Transform::Normal,
            )?;
            frame.clear(Color32F::new(0.0, 0.0, 0.0, 0.0), &damage)?;
            let _ = draw_render_elements(&mut frame, scale, &elements, &damage)?;
            let _ = frame.finish()?;
        }

        Ok(Some(Self {
            buffer: TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None),
            bbox,
        }))
    }

    /// Converts this snapshot into a closing window overlay.
    pub fn into_closing_window(
        self,
        element_location: Point<i32, Logical>,
        geometry_location: Point<i32, Logical>,
        monitor: TransactionMonitor,
    ) -> ClosingWindow {
        ClosingWindow {
            buffer: self.buffer,
            location: element_location + self.bbox.loc - geometry_location,
            size: self.bbox.size,
            monitor,
        }
    }

    /// Converts this snapshot into a resizing window overlay.
    pub fn into_resizing_window(
        &self,
        target_location: Point<i32, Logical>,
        current_geometry_location: Point<i32, Logical>,
        current_geometry_size: Size<i32, Logical>,
        target_geometry_size: Size<i32, Logical>,
        monitor: TransactionMonitor,
    ) -> ResizingWindow {
        let decoration_width = self.bbox.size.w - current_geometry_size.w;
        let decoration_height = self.bbox.size.h - current_geometry_size.h;
        let target_width = (target_geometry_size.w + decoration_width).max(1);
        let target_height = if target_geometry_size.h == current_geometry_size.h {
            self.bbox.size.h
        } else {
            (target_geometry_size.h + decoration_height).max(1)
        };

        ResizingWindow {
            buffer: self.buffer.clone(),
            location: target_location + self.bbox.loc - current_geometry_location,
            size: Size::from((target_width, target_height)),
            monitor,
        }
    }
}

impl ClosingWindow {
    /// Checks if this window has finished its close animation.
    pub fn is_finished(&self) -> bool {
        self.monitor.is_released()
    }

    pub fn render_element(&self) -> RenderElements {
        RenderElements::from(TextureRenderElement::from_texture_buffer(
            self.location.to_f64().to_physical_precise_round(Scale::from(1.0)),
            &self.buffer,
            Some(1.0),
            None,
            Some(self.size),
            smithay::backend::renderer::element::Kind::Unspecified,
        ))
    }
}

impl ResizingWindow {
    /// Checks if this resize overlay has finished.
    pub fn is_finished(&self) -> bool {
        self.monitor.is_released()
    }

    pub fn render_element(&self) -> RenderElements {
        RenderElements::from(TextureRenderElement::from_texture_buffer(
            self.location.to_f64().to_physical_precise_round(Scale::from(1.0)),
            &self.buffer,
            Some(1.0),
            None,
            Some(self.size),
            smithay::backend::renderer::element::Kind::Unspecified,
        ))
    }
}
