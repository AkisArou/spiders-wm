use std::{cell::OnceCell, rc::Rc};

use anyhow::{anyhow, bail};
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Frame, Offscreen, Renderer, RendererSuper,
            element::{
                self, RenderElement,
                solid::{SolidColorBuffer, SolidColorRenderElement},
                surface::{WaylandSurfaceRenderElement, WaylandSurfaceTexture},
                texture::{TextureBuffer, TextureRenderElement},
                utils::{Relocate, RelocateRenderElement, RescaleRenderElement},
            },
            gles::{GlesRenderer, GlesTexture},
            sync::SyncPoint,
            utils::RendererSurfaceStateUserData,
        },
    },
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    utils::{Physical, Point, Rectangle, Scale, Size, Transform},
    wayland::compositor::{self, TraversalAction},
};
use tracing::{debug, warn};

smithay::backend::renderer::element::render_elements! {
    #[derive(Debug)]
    pub SurfaceTextureRenderElement<R> where R: Renderer;
    Texture = TextureRenderElement<R::TextureId>,
    SolidColor = SolidColorRenderElement,
}

pub(crate) type SnapshotRenderElement =
    RescaleRenderElement<SurfaceTextureRenderElement<GlesRenderer>>;

pub(crate) fn memory_render_element(
    renderer: &mut GlesRenderer,
    location: Point<i32, Physical>,
    scale: Scale<f64>,
    alpha: f32,
    bytes: &[u8],
    width: i32,
    height: i32,
) -> Option<SnapshotRenderElement> {
    let texture = TextureBuffer::from_memory(
        renderer,
        bytes,
        Fourcc::Argb8888,
        (width, height),
        false,
        1,
        Transform::Normal,
        None,
    )
    .ok()?;
    let element = TextureRenderElement::from_texture_buffer(
        location.to_f64(),
        &texture,
        Some(alpha),
        None,
        None,
        element::Kind::Unspecified,
    );

    Some(RescaleRenderElement::from_element(
        SurfaceTextureRenderElement::Texture(element),
        location,
        Scale::from((1.0 / scale.x, 1.0 / scale.y)),
    ))
}

#[derive(Debug, Clone)]
struct EncompassingTexture {
    texture: GlesTexture,
    _sync_point: SyncPoint,
    loc: Point<i32, Physical>,
}

#[derive(Debug, Clone)]
pub(crate) struct WindowSnapshot {
    elements: Rc<Vec<SurfaceTextureRenderElement<GlesRenderer>>>,
    texture: OnceCell<(GlesTexture, Point<i32, Physical>)>,
}

impl WindowSnapshot {
    pub(crate) fn capture(
        renderer: &mut GlesRenderer,
        surface: &WlSurface,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Option<Self> {
        let elements =
            texture_render_elements_from_surface_tree(renderer, surface, (0, 0), scale, alpha);
        if elements.is_empty() {
            return None;
        }

        Some(Self { elements: Rc::new(elements), texture: OnceCell::new() })
    }

    pub(crate) fn render_element(
        &self,
        renderer: &mut GlesRenderer,
        location: Point<i32, Physical>,
        scale: Scale<f64>,
        alpha: f32,
    ) -> Option<SnapshotRenderElement> {
        let (texture, offset) = self.texture(renderer)?;
        let loc = location + offset;
        let buffer = TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None);
        let element = TextureRenderElement::from_texture_buffer(
            loc.to_f64(),
            &buffer,
            Some(alpha),
            None,
            None,
            element::Kind::Unspecified,
        );

        Some(RescaleRenderElement::from_element(
            SurfaceTextureRenderElement::Texture(element),
            loc,
            Scale::from((1.0 / scale.x, 1.0 / scale.y)),
        ))
    }

    fn texture(&self, renderer: &mut GlesRenderer) -> Option<(GlesTexture, Point<i32, Physical>)> {
        if self.texture.get().is_none() {
            let EncompassingTexture { texture, _sync_point, loc } =
                match render_to_encompassing_texture(
                    renderer,
                    self.elements.iter(),
                    Scale::from((1.0, 1.0)),
                    Transform::Normal,
                    Fourcc::Argb8888,
                ) {
                    Ok(texture) => texture,
                    Err(error) => {
                        debug!(?error, "wm failed to render close snapshot to texture");
                        return None;
                    }
                };

            let Ok(()) = self.texture.set((texture, loc)) else {
                unreachable!();
            };
        }

        self.texture.get().cloned()
    }
}

fn texture_render_elements_from_surface_tree(
    renderer: &mut GlesRenderer,
    surface: &WlSurface,
    location: impl Into<Point<i32, Physical>>,
    scale: impl Into<Scale<f64>>,
    alpha: f32,
) -> Vec<SurfaceTextureRenderElement<GlesRenderer>> {
    let location = location.into().to_f64();
    let scale = scale.into();
    let mut surfaces = Vec::new();

    compositor::with_surface_tree_downward(
        surface,
        location,
        |_, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                let data = data.lock().expect("renderer surface state poisoned");
                if let Some(view) = data.view() {
                    location += view.offset.to_f64().to_physical(scale);
                    TraversalAction::DoChildren(location)
                } else {
                    TraversalAction::SkipChildren
                }
            } else {
                TraversalAction::SkipChildren
            }
        },
        |surface, states, location| {
            let mut location = *location;
            let data = states.data_map.get::<RendererSurfaceStateUserData>();

            if let Some(data) = data {
                let has_view = {
                    let data = data.lock().expect("renderer surface state poisoned");
                    if let Some(view) = data.view() {
                        location += view.offset.to_f64().to_physical(scale);
                        true
                    } else {
                        false
                    }
                };

                if has_view {
                    match WaylandSurfaceRenderElement::from_surface(
                        renderer,
                        surface,
                        states,
                        location,
                        alpha,
                        element::Kind::Unspecified,
                    ) {
                        Ok(Some(surface_element)) => {
                            let data = data.lock().expect("renderer surface state poisoned");
                            let view = data.view().expect("surface view missing");

                            match surface_element.texture() {
                                WaylandSurfaceTexture::Texture(texture) => {
                                    let texture_buffer = TextureBuffer::from_texture(
                                        renderer,
                                        texture.clone(),
                                        data.buffer_scale(),
                                        data.buffer_transform(),
                                        None,
                                    );
                                    let texture_element = TextureRenderElement::from_texture_buffer(
                                        location,
                                        &texture_buffer,
                                        Some(alpha),
                                        Some(view.src),
                                        Some(view.dst),
                                        element::Kind::Unspecified,
                                    );
                                    surfaces.push(SurfaceTextureRenderElement::Texture(
                                        texture_element,
                                    ));
                                }
                                WaylandSurfaceTexture::SolidColor(color) => {
                                    let solid_color_buffer =
                                        SolidColorBuffer::new(view.dst, *color);
                                    let solid_color = SolidColorRenderElement::from_buffer(
                                        &solid_color_buffer,
                                        location.to_i32_round(),
                                        scale,
                                        alpha,
                                        element::Kind::Unspecified,
                                    );
                                    surfaces
                                        .push(SurfaceTextureRenderElement::SolidColor(solid_color));
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            warn!(%error, "wm failed to import surface for close snapshot");
                        }
                    }
                }
            }
        },
        |_, _, _| true,
    );

    surfaces
}

fn render_to_encompassing_texture<E: RenderElement<GlesRenderer>>(
    renderer: &mut GlesRenderer,
    elements: impl IntoIterator<Item = E>,
    scale: Scale<f64>,
    transform: Transform,
    fourcc: Fourcc,
) -> anyhow::Result<EncompassingTexture> {
    let elements = elements.into_iter().collect::<Vec<_>>();
    let encompassing_geo = elements
        .iter()
        .map(|element| element.geometry(scale))
        .reduce(|first, second| first.merge(second))
        .ok_or_else(|| anyhow!("no elements to render"))?;

    let relocated = elements.iter().rev().map(|element| {
        RelocateRenderElement::from_element(
            element,
            (-encompassing_geo.loc.x, -encompassing_geo.loc.y),
            Relocate::Relative,
        )
    });

    let (texture, sync_point) =
        render_to_texture(renderer, relocated, encompassing_geo.size, scale, transform, fourcc)?;

    Ok(EncompassingTexture { texture, _sync_point: sync_point, loc: encompassing_geo.loc })
}

fn render_to_texture(
    renderer: &mut GlesRenderer,
    elements: impl IntoIterator<Item = impl RenderElement<GlesRenderer>>,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
    fourcc: Fourcc,
) -> anyhow::Result<(GlesTexture, SyncPoint)> {
    if size.is_empty() {
        bail!("size was empty");
    }

    let buffer_size = size.to_logical(1).to_buffer(1, Transform::Normal);
    let mut texture = renderer.create_buffer(fourcc, buffer_size)?;

    let sync_point = {
        let mut framebuffer = renderer.bind(&mut texture)?;
        render_elements_to_framebuffer(
            renderer,
            &mut framebuffer,
            elements,
            size,
            scale,
            transform,
        )?
    };

    Ok((texture, sync_point))
}

fn render_elements_to_framebuffer(
    renderer: &mut GlesRenderer,
    framebuffer: &mut <GlesRenderer as RendererSuper>::Framebuffer<'_>,
    elements: impl IntoIterator<Item = impl RenderElement<GlesRenderer>>,
    size: Size<i32, Physical>,
    scale: Scale<f64>,
    transform: Transform,
) -> anyhow::Result<SyncPoint> {
    let dst_rect = Rectangle::from_size(transform.transform_size(size));
    let mut frame = renderer.render(framebuffer, size, transform)?;
    frame.clear([0.0, 0.0, 0.0, 0.0].into(), &[dst_rect])?;

    for element in elements {
        let src = element.src();
        let dst = element.geometry(scale);
        if let Some(mut damage) = dst_rect.intersection(dst) {
            damage.loc -= dst.loc;
            element.draw(&mut frame, src, dst, &[damage], &[])?;
        }
    }

    frame.finish().map_err(Into::into)
}
