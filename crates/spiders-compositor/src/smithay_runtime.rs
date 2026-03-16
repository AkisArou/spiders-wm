#[cfg(feature = "smithay-winit")]
mod imp {
    use std::sync::Arc;
    use std::time::Duration;

    use font8x8::{BASIC_FONTS, UnicodeFonts};
    use std::collections::{HashMap, HashSet, VecDeque};

    use smithay::backend::allocator::Fourcc;
    use smithay::backend::input::{
        AbsolutePositionEvent, ButtonState, Event, InputEvent, KeyboardKeyEvent, PointerButtonEvent,
    };
    use smithay::backend::renderer::Color32F;
    use smithay::backend::renderer::Frame;
    use smithay::backend::renderer::damage::OutputDamageTracker;
    use smithay::backend::renderer::element::memory::{
        MemoryRenderBuffer, MemoryRenderBufferRenderElement,
    };
    use smithay::backend::renderer::element::solid::SolidColorRenderElement;
    use smithay::backend::renderer::element::surface::{
        WaylandSurfaceRenderElement, render_elements_from_surface_tree,
    };
    use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
    use smithay::backend::renderer::element::utils::{
        CropRenderElement, Relocate, RelocateRenderElement, RescaleRenderElement,
    };
    use smithay::backend::renderer::element::{Element, Id, Kind};
    use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
    use smithay::backend::renderer::sync::SyncPoint;
    use smithay::backend::renderer::utils::RendererSurfaceStateUserData;
    use smithay::backend::renderer::{
        Bind, ImportAll, ImportMem, Offscreen, Renderer, RendererSuper,
    };
    use smithay::backend::winit::{self, WinitEvent, WinitEventLoop, WinitGraphicsBackend};
    use smithay::desktop::utils::{
        OutputPresentationFeedback, send_frames_surface_tree,
        surface_presentation_feedback_flags_from_states, take_presentation_feedback_surface_tree,
    };
    use smithay::input::keyboard::{FilterResult, Keysym, ModifiersState, xkb};
    use smithay::input::pointer::CursorIcon;
    use smithay::input::pointer::{ButtonEvent, MotionEvent};
    use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
    use smithay::reexports::calloop::generic::Generic;
    use smithay::reexports::calloop::{
        EventLoop, Interest, LoopSignal, Mode as CalloopMode, PostAction,
    };
    use smithay::reexports::wayland_protocols::wp::presentation_time::server::wp_presentation_feedback;
    use smithay::reexports::wayland_server::Display;
    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
    use smithay::utils::{Clock, Monotonic, Point, Rectangle, SERIAL_COUNTER, Transform};
    use smithay::wayland::compositor::{TraversalAction, with_states, with_surface_tree_downward};
    use smithay::wayland::presentation::Refresh;
    use spiders_config::model::Config;
    use spiders_shared::api::WmAction;
    use spiders_shared::ids::{OutputId, WindowId};
    use spiders_shared::layout::LayoutRect;
    use spiders_shared::runtime::AuthoringLayoutRuntime;
    use spiders_shared::wm::OutputSnapshot;
    use spiders_wm::{
        CompositorTopologyState, ControllerCommand, ControllerReport, OutputState, SeatState,
        SurfaceState,
    };

    use crate::smithay_adapter::{SmithayAdapter, SmithayAdapterEvent, SmithaySeatDescriptor};
    use crate::smithay_state::{
        SmithayClientState, SmithayRenderableToplevelSurface, SmithayStateError,
        SmithayStateSnapshot, SmithayWindowDecorationPolicySnapshot, SmithayWindowRenderSnapshot,
        SpidersSmithayState,
    };
    use crate::titlebar::TitlebarRenderItem;
    use crate::transitions::{
        ClosingWindowTransition, OpeningWindowTransition, ResizeTransition, SceneTextureSnapshot,
        SceneTransition, TransitionTextureSnapshot, now as transition_now,
    };

    fn append_winit_debug_log(message: &str) {
        let Some(path) = std::env::var_os("SPIDERS_WM_WINIT_DEBUG_LOG_PATH") else {
            return;
        };

        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| {
                use std::io::Write;
                writeln!(file, "{message}")
            });
    }

    smithay::backend::renderer::element::render_elements! {
        CompositorRenderElement<R> where R: ImportAll + ImportMem;
        Solid = SolidColorRenderElement,
        Memory = MemoryRenderBufferRenderElement<R>,
        Surface = WaylandSurfaceRenderElement<R>,
        ClippedSurface = CropRenderElement<WaylandSurfaceRenderElement<R>>,
        ResizedSurface = CropRenderElement<RelocateRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<R>>>>,
        SnapshotSurface = CropRenderElement<RelocateRenderElement<RescaleRenderElement<TextureRenderElement<<R as smithay::backend::renderer::RendererSuper>::TextureId>>>>,
    }

    const DEFAULT_CLEAR_COLOR: [f32; 4] = [0.08, 0.08, 0.09, 1.0];
    const BTN_LEFT: u32 = 0x110;
    const CLOSE_TRANSITION_DURATION: Duration = Duration::from_millis(220);
    const OPEN_TRANSITION_DURATION: Duration = Duration::from_millis(140);
    const RESIZE_TRANSITION_DURATION: Duration = Duration::from_millis(180);

    #[derive(Debug)]
    struct TitlebarRenderState {
        damage_tracker: OutputDamageTracker,
    }

    #[derive(Debug)]
    struct PresentationRenderState {
        clock: Clock<Monotonic>,
    }

    #[derive(Debug, Clone)]
    struct FrameDebugRecord {
        frame_index: u64,
        rendered: bool,
        reason: String,
        focused_window_id: Option<WindowId>,
        presented_window_ids: Vec<WindowId>,
        pending_presented_window_ids: Vec<WindowId>,
        scene_transaction: Option<FrameSceneTransactionDebugRecord>,
        windows: Vec<FrameWindowDebugRecord>,
    }

    #[derive(Debug, Clone)]
    struct FrameSceneTransactionDebugRecord {
        affected_windows: Vec<WindowId>,
        frozen_rect: (i32, i32, i32, i32),
        alpha_milli: i32,
        layout_changed: bool,
        settled: bool,
        emitted_overlay: bool,
    }

    #[derive(Debug, Clone)]
    struct FrameWindowDebugRecord {
        window_id: WindowId,
        target_rect: (i32, i32, i32, i32),
        committed_size: Option<(i32, i32)>,
        committed_view_size: Option<(i32, i32)>,
        has_buffer: bool,
        has_resize_snapshot: bool,
        snapshot_rect: Option<(i32, i32, i32, i32)>,
        considered_presented: bool,
        pending_presented: bool,
        drew_live: bool,
        drew_snapshot: bool,
    }

    const FRAME_DEBUG_HISTORY_LIMIT: usize = 256;

    impl PresentationRenderState {
        fn new() -> Self {
            Self {
                clock: Clock::new(),
            }
        }
    }

    impl TitlebarRenderState {
        fn new(output: &Output) -> Self {
            Self {
                damage_tracker: OutputDamageTracker::from_output(output),
            }
        }

        fn next_element_id(&mut self) -> Id {
            Id::new()
        }
    }

    fn render_snapshot_elements(
        renderer: &mut GlesRenderer,
        target: &mut smithay::backend::renderer::gles::GlesTarget<'_>,
        size: smithay::utils::Size<i32, smithay::utils::Physical>,
        scale: smithay::utils::Scale<f64>,
        transform: Transform,
        elements: impl Iterator<
            Item = impl smithay::backend::renderer::element::RenderElement<GlesRenderer>,
        >,
    ) -> Result<SyncPoint, SmithayRuntimeError> {
        let transform = transform.invert();
        let output_rect = Rectangle::from_size(transform.transform_size(size));

        let mut frame = renderer.render(target, size, transform).map_err(
            |err: <GlesRenderer as RendererSuper>::Error| {
                SmithayRuntimeError::Winit(err.to_string())
            },
        )?;

        frame.clear(Color32F::TRANSPARENT, &[output_rect]).map_err(
            |err: <GlesRenderer as RendererSuper>::Error| {
                SmithayRuntimeError::Winit(err.to_string())
            },
        )?;

        for element in elements {
            let src = element.src();
            let dst = element.geometry(scale);

            if let Some(mut damage) = output_rect.intersection(dst) {
                damage.loc -= dst.loc;
                element.draw(&mut frame, src, dst, &[damage], &[]).map_err(
                    |err: <GlesRenderer as RendererSuper>::Error| {
                        SmithayRuntimeError::Winit(err.to_string())
                    },
                )?;
            }
        }

        frame
            .finish()
            .map_err(|err: <GlesRenderer as RendererSuper>::Error| {
                SmithayRuntimeError::Winit(err.to_string())
            })
    }

    fn capture_window_snapshot(
        renderer: &mut GlesRenderer,
        surface: &SmithayRenderableToplevelSurface,
        render_snapshot: &SmithayWindowRenderSnapshot,
        output_scale: smithay::utils::Scale<f64>,
    ) -> Result<Option<TransitionTextureSnapshot>, SmithayRuntimeError> {
        let surface_elements = render_elements_from_surface_tree::<
            GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>,
        >(
            renderer,
            surface.surface.wl_surface(),
            (0, 0),
            output_scale,
            1.0,
            Kind::Unspecified,
        );

        let mut bounds: Option<Rectangle<i32, smithay::utils::Physical>> = None;
        for element in &surface_elements {
            let geometry = element.geometry(output_scale);
            bounds = Some(match bounds {
                Some(existing) => existing.merge(geometry),
                None => geometry,
            });
        }

        let Some(bounds) = bounds else {
            return Ok(None);
        };
        if bounds.size.w <= 0 || bounds.size.h <= 0 {
            return Ok(None);
        }

        let mut texture = renderer
            .create_buffer(
                Fourcc::Abgr8888,
                bounds.size.to_logical(1).to_buffer(1, Transform::Normal),
            )
            .map_err(|err| SmithayRuntimeError::Winit(err.to_string()))?;
        let relocated = surface_elements.into_iter().map(|element| {
            RelocateRenderElement::from_element(element, bounds.loc.upscale(-1), Relocate::Relative)
        });
        {
            let mut target = renderer
                .bind(&mut texture)
                .map_err(|err| SmithayRuntimeError::Winit(err.to_string()))?;
            let _ = render_snapshot_elements(
                renderer,
                &mut target,
                bounds.size,
                output_scale,
                Transform::Normal,
                relocated,
            )?;
        }

        Ok(Some(TransitionTextureSnapshot {
            buffer: TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None),
            render_snapshot: render_snapshot.clone(),
            logical_size: (
                bounds.size.w as f64 / output_scale.x.max(1.0),
                bounds.size.h as f64 / output_scale.y.max(1.0),
            ),
        }))
    }

    fn capture_scene_snapshot(
        renderer: &mut GlesRenderer,
        windows: &[SmithayWindowRenderSnapshot],
        titlebars: &[TitlebarRenderItem],
        decoration_policies: &[(WindowId, SmithayWindowDecorationPolicySnapshot)],
        surfaces: &[SmithayRenderableToplevelSurface],
        output_size: smithay::utils::Size<i32, smithay::utils::Physical>,
        output_scale: smithay::utils::Scale<f64>,
    ) -> Result<Option<SceneTextureSnapshot>, SmithayRuntimeError> {
        if output_size.w <= 0 || output_size.h <= 0 {
            return Ok(None);
        }

        let mut scene_elements: Vec<CompositorRenderElement<GlesRenderer>> = Vec::new();
        scene_elements.extend(
            windows
                .iter()
                .flat_map(|window| build_window_border_elements(window, decoration_policies))
                .map(CompositorRenderElement::from),
        );
        for window in windows {
            let Some(surface) = surfaces
                .iter()
                .find(|surface| surface.window_id == window.window_id)
            else {
                continue;
            };
            let location = (
                window.window_rect.x.round() as i32,
                (window.window_rect.y + window.content_offset_y).round() as i32,
            );
            scene_elements.extend(
                render_elements_from_surface_tree::<
                    GlesRenderer,
                    WaylandSurfaceRenderElement<GlesRenderer>,
                >(
                    renderer,
                    surface.surface.wl_surface(),
                    location,
                    output_scale,
                    1.0,
                    Kind::Unspecified,
                )
                .into_iter()
                .map(CompositorRenderElement::from),
            );
        }
        scene_elements.extend(
            titlebars
                .iter()
                .filter_map(build_titlebar_border_element)
                .map(CompositorRenderElement::from),
        );
        scene_elements.extend(
            titlebars
                .iter()
                .filter_map(|item| build_titlebar_text_element(renderer, item))
                .map(CompositorRenderElement::from),
        );

        let mut texture = renderer
            .create_buffer(
                Fourcc::Abgr8888,
                output_size.to_logical(1).to_buffer(1, Transform::Normal),
            )
            .map_err(|err| SmithayRuntimeError::Winit(err.to_string()))?;
        {
            let mut target = renderer
                .bind(&mut texture)
                .map_err(|err| SmithayRuntimeError::Winit(err.to_string()))?;
            let _ = render_snapshot_elements(
                renderer,
                &mut target,
                output_size,
                output_scale,
                Transform::Normal,
                scene_elements.into_iter(),
            )?;
        }

        Ok(Some(SceneTextureSnapshot {
            buffer: TextureBuffer::from_texture(renderer, texture, 1, Transform::Normal, None),
            logical_size: (
                output_size.w as f64 / output_scale.x.max(1.0),
                output_size.h as f64 / output_scale.y.max(1.0),
            ),
        }))
    }

    fn committed_root_surface_size(surface: &WlSurface) -> Option<(i32, i32)> {
        let mut max_width = 0;
        let mut max_height = 0;
        let mut saw_view = false;

        with_surface_tree_downward(
            surface,
            Point::<f64, smithay::utils::Logical>::from((0.0, 0.0)),
            |_, states, location| {
                let mut location = *location;
                let data = states.data_map.get::<RendererSurfaceStateUserData>();

                if let Some(data) = data {
                    if let Some(view) = data.lock().unwrap().view() {
                        location += view.offset.to_f64();
                        TraversalAction::DoChildren(location)
                    } else {
                        TraversalAction::SkipChildren
                    }
                } else {
                    TraversalAction::SkipChildren
                }
            },
            |_, states, location| {
                let mut location = *location;
                let data = states.data_map.get::<RendererSurfaceStateUserData>();
                let Some(data) = data else {
                    return;
                };
                let data = data.lock().unwrap();
                let Some(view) = data.view() else {
                    return;
                };

                location += view.offset.to_f64();
                saw_view = true;
                max_width = max_width.max((location.x + f64::from(view.dst.w)).round() as i32);
                max_height = max_height.max((location.y + f64::from(view.dst.h)).round() as i32);
            },
            |_, _, _| true,
        );

        saw_view.then_some((max_width.max(0), max_height.max(0)))
    }

    fn ready_for_initial_presentation(
        committed_size: Option<(i32, i32)>,
        has_live_surface_content: bool,
        expected_size: (i32, i32),
    ) -> bool {
        let (expected_width, expected_height) = expected_size;
        if expected_width <= 0 || expected_height <= 0 {
            return false;
        }

        has_live_surface_content
            && matches!(committed_size, Some((width, height)) if width > 0 && height > 0)
    }

    fn build_snapshot_fallback_element(
        snapshot: &TransitionTextureSnapshot,
        alpha: f32,
        output_scale: smithay::utils::Scale<f64>,
        target_location: Option<(i32, i32)>,
        target_size: Option<(i32, i32)>,
    ) -> Option<
        CropRenderElement<
            RelocateRenderElement<
                RescaleRenderElement<
                    TextureRenderElement<
                        <GlesRenderer as smithay::backend::renderer::RendererSuper>::TextureId,
                    >,
                >,
            >,
        >,
    > {
        let texture = TextureRenderElement::from_texture_buffer(
            Point::from((0.0, 0.0)),
            &snapshot.buffer,
            Some(alpha),
            None,
            Some(
                (
                    snapshot.logical_size.0.round() as i32,
                    snapshot.logical_size.1.round() as i32,
                )
                    .into(),
            ),
            Kind::Unspecified,
        );
        let source_size = (
            snapshot.logical_size.0.round().max(0.0) as i32,
            snapshot.logical_size.1.round().max(0.0) as i32,
        );
        let render_size = target_size.map_or(source_size, |(target_width, target_height)| {
            (
                source_size.0.min(target_width.max(0)),
                source_size.1.min(target_height.max(0)),
            )
        });
        let element = RescaleRenderElement::from_element(texture, Point::from((0, 0)), (1.0, 1.0));
        let render_location = target_location.unwrap_or((
            snapshot.render_snapshot.window_rect.x.round() as i32,
            (snapshot.render_snapshot.window_rect.y + snapshot.render_snapshot.content_offset_y)
                .round() as i32,
        ));
        let element =
            RelocateRenderElement::from_element(element, render_location, Relocate::Absolute);
        let constrain_rect = Rectangle::new(render_location.into(), render_size.into());
        CropRenderElement::from_element(element, output_scale, constrain_rect)
    }

    fn build_scene_snapshot_element(
        snapshot: &SceneTextureSnapshot,
        alpha: f32,
        output_scale: smithay::utils::Scale<f64>,
        frozen_rect: LayoutRect,
    ) -> Option<
        CropRenderElement<
            RelocateRenderElement<
                RescaleRenderElement<
                    TextureRenderElement<
                        <GlesRenderer as smithay::backend::renderer::RendererSuper>::TextureId,
                    >,
                >,
            >,
        >,
    > {
        let texture = TextureRenderElement::from_texture_buffer(
            Point::from((0.0, 0.0)),
            &snapshot.buffer,
            Some(alpha),
            None,
            Some(
                (
                    snapshot.logical_size.0.round() as i32,
                    snapshot.logical_size.1.round() as i32,
                )
                    .into(),
            ),
            Kind::Unspecified,
        );
        let element = RescaleRenderElement::from_element(texture, Point::from((0, 0)), (1.0, 1.0));
        let render_location = (frozen_rect.x.round() as i32, frozen_rect.y.round() as i32);
        let element =
            RelocateRenderElement::from_element(element, render_location, Relocate::Absolute);
        let constrain_rect = Rectangle::new(
            render_location.into(),
            (
                frozen_rect.width.round().max(0.0) as i32,
                frozen_rect.height.round().max(0.0) as i32,
            )
                .into(),
        );
        CropRenderElement::from_element(element, output_scale, constrain_rect)
    }

    fn window_ids(windows: &[SmithayWindowRenderSnapshot]) -> Vec<WindowId> {
        windows
            .iter()
            .map(|window| window.window_id.clone())
            .collect()
    }

    fn union_window_rect(windows: &[SmithayWindowRenderSnapshot]) -> Option<LayoutRect> {
        let first = windows.first()?;
        let mut min_x = first.window_rect.x;
        let mut min_y = first.window_rect.y;
        let mut max_x = first.window_rect.x + first.window_rect.width;
        let mut max_y = first.window_rect.y + first.window_rect.height;

        for window in &windows[1..] {
            min_x = min_x.min(window.window_rect.x);
            min_y = min_y.min(window.window_rect.y);
            max_x = max_x.max(window.window_rect.x + window.window_rect.width);
            max_y = max_y.max(window.window_rect.y + window.window_rect.height);
        }

        Some(LayoutRect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        })
    }

    fn union_layout_rects(a: LayoutRect, b: LayoutRect) -> LayoutRect {
        let min_x = a.x.min(b.x);
        let min_y = a.y.min(b.y);
        let max_x = (a.x + a.width).max(b.x + b.width);
        let max_y = (a.y + a.height).max(b.y + b.height);

        LayoutRect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        }
    }

    fn overlap_1d(a_start: f32, a_end: f32, b_start: f32, b_end: f32) -> f32 {
        (a_end.min(b_end) - a_start.max(b_start)).max(0.0)
    }

    fn projected_close_affected_window_ids(
        windows: &[SmithayWindowRenderSnapshot],
        closing_window_id: &WindowId,
    ) -> Vec<WindowId> {
        let Some(closing_window) = windows
            .iter()
            .find(|window| window.window_id == *closing_window_id)
        else {
            return vec![closing_window_id.clone()];
        };

        let closed_rect = closing_window.window_rect;
        let mut same_column = Vec::new();
        let mut side_overlap = Vec::new();

        for window in windows {
            if window.window_id == *closing_window_id {
                continue;
            }

            let candidate = window.window_rect;
            let horizontal_overlap = overlap_1d(
                closed_rect.x,
                closed_rect.x + closed_rect.width,
                candidate.x,
                candidate.x + candidate.width,
            );
            let vertical_overlap = overlap_1d(
                closed_rect.y,
                closed_rect.y + closed_rect.height,
                candidate.y,
                candidate.y + candidate.height,
            );

            if horizontal_overlap > 0.0 {
                same_column.push(window.window_id.clone());
            } else if vertical_overlap > 0.0 {
                side_overlap.push(window.window_id.clone());
            }
        }

        let mut affected = vec![closing_window_id.clone()];
        if same_column.is_empty() {
            affected.extend(side_overlap);
        } else {
            affected.extend(same_column);
        }
        affected.sort();
        affected.dedup();
        affected
    }

    fn build_close_scene_transition(
        renderer: &mut GlesRenderer,
        windows: &[SmithayWindowRenderSnapshot],
        titlebars: &[crate::titlebar::TitlebarRenderItem],
        decoration_policies: &[(WindowId, SmithayWindowDecorationPolicySnapshot)],
        surfaces: &[SmithayRenderableToplevelSurface],
        window_id: &WindowId,
        started_at: std::time::Duration,
        window_size: (i32, i32),
        output_scale: smithay::utils::Scale<f64>,
    ) -> Option<SceneTransition> {
        let Some(render) = windows
            .iter()
            .find(|render| render.window_id == *window_id)
            .cloned()
        else {
            return None;
        };

        let Ok(Some(scene_snapshot)) = capture_scene_snapshot(
            renderer,
            windows,
            titlebars,
            decoration_policies,
            surfaces,
            window_size.into(),
            output_scale,
        ) else {
            return None;
        };

        let affected_windows = projected_close_affected_window_ids(windows, window_id);
        let affected_previous_windows = select_windows_by_ids(windows, &affected_windows);
        Some(SceneTransition::new(
            scene_snapshot,
            started_at,
            layout_signature(windows),
            affected_windows,
            union_window_rect(&affected_previous_windows).unwrap_or(render.window_rect),
        ))
    }

    fn merge_scene_transition(
        existing: Option<SceneTransition>,
        next: SceneTransition,
    ) -> SceneTransition {
        let Some(existing) = existing else {
            return next;
        };

        let overlaps = existing
            .affected_windows
            .iter()
            .any(|window_id| next.affected_windows.contains(window_id));
        if !overlaps {
            return next;
        }

        let mut existing = existing;
        existing.awaiting_layout_change = true;
        existing.awaiting_settle = true;
        existing.frozen_rect = union_layout_rects(existing.frozen_rect, next.frozen_rect);

        for window_id in next.affected_windows {
            if !existing.affected_windows.contains(&window_id) {
                existing.affected_windows.push(window_id.clone());
            }
            if !existing.blocked_windows.contains(&window_id) {
                existing.blocked_windows.push(window_id);
            }
        }
        existing.affected_windows.sort();
        existing.affected_windows.dedup();

        for window_id in next.blocked_windows {
            if !existing.blocked_windows.contains(&window_id) {
                existing.blocked_windows.push(window_id);
            }
        }
        existing.blocked_windows.sort();
        existing.blocked_windows.dedup();

        existing
    }

    fn scene_transition_has_visible_alpha(
        transition: &SceneTransition,
        windows: &[SmithayWindowRenderSnapshot],
        pending_presented_window_ids: &std::collections::HashSet<WindowId>,
        now: std::time::Duration,
    ) -> bool {
        let layout_changed = layout_signature(windows) != transition.captured_layouts;
        let settled = transition.blocked_windows.is_empty()
            && !transition
                .affected_windows
                .iter()
                .any(|window_id| pending_presented_window_ids.contains(window_id));
        scene_transition_alpha(now, transition, layout_changed, settled) > 0.0
    }

    fn widen_scene_transition_for_close(
        transition: &mut SceneTransition,
        windows: &[SmithayWindowRenderSnapshot],
        closing_window_id: &WindowId,
        now: std::time::Duration,
    ) {
        let affected_windows = projected_close_affected_window_ids(windows, closing_window_id);
        let affected_previous_windows = select_windows_by_ids(windows, &affected_windows);

        for window_id in affected_windows {
            if !transition.affected_windows.contains(&window_id) {
                transition.affected_windows.push(window_id.clone());
            }
            if !transition.blocked_windows.contains(&window_id) {
                transition.blocked_windows.push(window_id);
            }
        }

        transition.affected_windows.sort();
        transition.affected_windows.dedup();
        transition.blocked_windows.sort();
        transition.blocked_windows.dedup();

        if let Some(frozen_rect) = union_window_rect(&affected_previous_windows) {
            transition.frozen_rect = union_layout_rects(transition.frozen_rect, frozen_rect);
        }

        transition.awaiting_layout_change = true;
        transition.awaiting_settle = true;
        transition.started_at = now;
    }

    fn affected_window_ids_for_transition(
        previous_windows: &[SmithayWindowRenderSnapshot],
        next_windows: &[SmithayWindowRenderSnapshot],
    ) -> Vec<WindowId> {
        let mut affected = Vec::new();

        for previous in previous_windows {
            match next_windows
                .iter()
                .find(|next| next.window_id == previous.window_id)
            {
                Some(next) if next == previous => {}
                _ => affected.push(previous.window_id.clone()),
            }
        }

        for next in next_windows {
            if previous_windows
                .iter()
                .all(|previous| previous.window_id != next.window_id)
            {
                affected.push(next.window_id.clone());
            }
        }

        affected.sort();
        affected.dedup();
        affected
    }

    fn select_windows_by_ids<'a>(
        windows: &'a [SmithayWindowRenderSnapshot],
        window_ids: &[WindowId],
    ) -> Vec<SmithayWindowRenderSnapshot> {
        windows
            .iter()
            .filter(|window| window_ids.contains(&window.window_id))
            .cloned()
            .collect()
    }

    fn scene_transaction_debug_record(
        transition: Option<&SceneTransition>,
        windows: &[SmithayWindowRenderSnapshot],
        surfaces: &[SmithayRenderableToplevelSurface],
        pending_presented_window_ids: &std::collections::HashSet<WindowId>,
        now: std::time::Duration,
        emitted_overlay: bool,
    ) -> Option<FrameSceneTransactionDebugRecord> {
        let transition = transition?;
        let layout_changed = layout_signature(windows) != transition.captured_layouts;
        let settled = transition.blocked_windows.is_empty()
            && !transition
                .affected_windows
                .iter()
                .any(|window_id| pending_presented_window_ids.contains(window_id));
        Some(FrameSceneTransactionDebugRecord {
            affected_windows: transition.affected_windows.clone(),
            frozen_rect: (
                transition.frozen_rect.x.round() as i32,
                transition.frozen_rect.y.round() as i32,
                transition.frozen_rect.width.round() as i32,
                transition.frozen_rect.height.round() as i32,
            ),
            alpha_milli: (scene_transition_alpha(now, transition, layout_changed, settled) * 1000.0)
                .round() as i32,
            layout_changed,
            settled,
            emitted_overlay,
        })
    }

    fn scene_transition_needs_overlay(
        transition: &SceneTransition,
        windows: &[SmithayWindowRenderSnapshot],
        surfaces: &[SmithayRenderableToplevelSurface],
        close_requested_window_ids: &std::collections::HashSet<WindowId>,
        layout_changed: bool,
    ) -> bool {
        if transition
            .affected_windows
            .iter()
            .any(|window_id| close_requested_window_ids.contains(window_id))
        {
            return true;
        }

        if layout_changed {
            return true;
        }

        transition.affected_windows.iter().any(|window_id| {
            let Some(_window) = windows.iter().find(|window| window.window_id == *window_id) else {
                return true;
            };
            let Some(surface) = surfaces
                .iter()
                .find(|surface| surface.window_id == *window_id)
            else {
                return true;
            };
            !surface.has_buffer
        })
    }

    fn refresh_scene_transition_blocked_windows(
        transition: &mut SceneTransition,
        windows: &[SmithayWindowRenderSnapshot],
        surfaces: &[SmithayRenderableToplevelSurface],
        presented_window_ids: &std::collections::HashSet<WindowId>,
        pending_presented_window_ids: &std::collections::HashSet<WindowId>,
    ) {
        transition.blocked_windows.retain(|window_id| {
            if pending_presented_window_ids.contains(window_id) {
                return true;
            }
            if !transition.affected_windows.contains(window_id) {
                return false;
            }
            let Some(window) = windows.iter().find(|window| window.window_id == *window_id) else {
                return false;
            };
            let Some(surface) = surfaces.iter().find(|surface| surface.window_id == *window_id)
            else {
                return false;
            };
            if !presented_window_ids.contains(window_id) {
                return true;
            }
            let expected_width = window.window_rect.width.max(0.0).round() as i32;
            let expected_height = (window.window_rect.height - window.content_offset_y)
                .max(0.0)
                .round() as i32;
            !matches!(
                committed_root_surface_size(surface.surface.wl_surface()).or(surface.committed_size),
                Some((width, height))
                    if (width - expected_width).abs() <= 1 && (height - expected_height).abs() <= 1
            )
        });
    }

    fn layout_signature(
        windows: &[SmithayWindowRenderSnapshot],
    ) -> Vec<(WindowId, (i32, i32, i32, i32))> {
        windows
            .iter()
            .map(|window| {
                (
                    window.window_id.clone(),
                    (
                        window.window_rect.x.round() as i32,
                        window.window_rect.y.round() as i32,
                        window.window_rect.width.round() as i32,
                        window.window_rect.height.round() as i32,
                    ),
                )
            })
            .collect()
    }

    fn closing_transition_is_active(
        now: std::time::Duration,
        transition: &ClosingWindowTransition,
    ) -> bool {
        now.saturating_sub(transition.started_at) < CLOSE_TRANSITION_DURATION
    }

    fn resize_transition_is_active(
        now: std::time::Duration,
        transition: &ResizeTransition,
    ) -> bool {
        now.saturating_sub(transition.started_at) < RESIZE_TRANSITION_DURATION
    }

    fn resize_transition_progress(now: std::time::Duration, transition: &ResizeTransition) -> f32 {
        if transition.awaiting_target_commit {
            return 0.0;
        }
        (now.saturating_sub(transition.started_at).as_secs_f32()
            / RESIZE_TRANSITION_DURATION.as_secs_f32())
        .clamp(0.0, 1.0)
    }

    fn committed_matches_transition_target(
        committed_size: Option<(i32, i32)>,
        transition: &ResizeTransition,
    ) -> bool {
        let Some((committed_width, committed_height)) = committed_size else {
            return false;
        };

        let (target_width, target_height) = transition.target_logical_size;
        if target_width <= 0 || target_height <= 0 {
            return committed_width > 0 && committed_height > 0;
        }

        let width_delta = (committed_width - target_width).abs();
        let height_delta = (committed_height - target_height).abs();
        width_delta <= 1 && height_delta <= 1
    }

    fn resize_transition_can_release(
        committed_size: Option<(i32, i32)>,
        transition: &ResizeTransition,
        now: std::time::Duration,
    ) -> bool {
        committed_matches_transition_target(committed_size, transition)
            && !transition.awaiting_target_commit
            && resize_transition_progress(now, transition) >= 1.0
    }

    fn closing_transition_alpha(
        now: std::time::Duration,
        transition: &ClosingWindowTransition,
    ) -> f32 {
        let progress = (now.saturating_sub(transition.started_at).as_secs_f32()
            / CLOSE_TRANSITION_DURATION.as_secs_f32())
        .clamp(0.0, 1.0);
        (1.0 - progress).max(0.0)
    }

    fn scene_transition_alpha(
        now: std::time::Duration,
        transition: &SceneTransition,
        layout_changed: bool,
        settled: bool,
    ) -> f32 {
        let _ = now;
        let _ = transition;
        if transition.awaiting_layout_change && !layout_changed {
            return 1.0;
        }
        if transition.awaiting_settle && !settled {
            return 1.0;
        }

        0.0
    }

    fn opening_transition_alpha(
        now: std::time::Duration,
        transition: &OpeningWindowTransition,
    ) -> f32 {
        let elapsed = now.saturating_sub(transition.first_seen_at);
        let progress =
            (elapsed.as_secs_f32() / OPEN_TRANSITION_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        progress.max(0.15)
    }

    #[derive(Debug, thiserror::Error)]
    pub enum SmithayRuntimeError {
        #[error("winit backend init failed: {0}")]
        Winit(String),
        #[error(transparent)]
        State(#[from] SmithayStateError),
        #[error(transparent)]
        Controller(#[from] crate::controller::ControllerCommandError),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayStartupReport {
        pub controller: ControllerReport,
        pub output_name: String,
        pub seat_name: String,
        pub logical_size: (i32, i32),
        pub socket_name: Option<String>,
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct SmithayWinitOptions {
        pub socket_name: Option<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayRuntimeSnapshot {
        pub socket_name: String,
        pub window_size: (i32, i32),
        pub state: SmithayStateSnapshot,
        pub presentation_debug: Vec<WindowPresentationDebugSnapshot>,
        pub frame_debug_history: Vec<FrameDebugSnapshot>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FrameDebugSnapshot {
        pub frame_index: u64,
        pub rendered: bool,
        pub reason: String,
        pub focused_window_id: Option<WindowId>,
        pub presented_window_ids: Vec<WindowId>,
        pub pending_presented_window_ids: Vec<WindowId>,
        pub scene_transaction: Option<FrameSceneTransactionDebugSnapshot>,
        pub windows: Vec<FrameWindowDebugSnapshot>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FrameSceneTransactionDebugSnapshot {
        pub affected_windows: Vec<WindowId>,
        pub frozen_rect: (i32, i32, i32, i32),
        pub alpha_milli: i32,
        pub layout_changed: bool,
        pub settled: bool,
        pub emitted_overlay: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct FrameWindowDebugSnapshot {
        pub window_id: WindowId,
        pub target_rect: (i32, i32, i32, i32),
        pub committed_size: Option<(i32, i32)>,
        pub committed_view_size: Option<(i32, i32)>,
        pub has_buffer: bool,
        pub has_resize_snapshot: bool,
        pub snapshot_rect: Option<(i32, i32, i32, i32)>,
        pub considered_presented: bool,
        pub pending_presented: bool,
        pub drew_live: bool,
        pub drew_snapshot: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayBootstrapSnapshot {
        pub runtime: SmithayRuntimeSnapshot,
        pub controller: ControllerReport,
        pub topology: SmithayBootstrapTopologySnapshot,
        pub runtime_bootstrap_debug: crate::app::RuntimeBootstrapDebug,
        pub lifecycle_debug: SmithayLifecycleDebugSnapshot,
        pub topology_surface_count: usize,
        pub topology_output_count: usize,
        pub topology_seat_count: usize,
    }

    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct SmithayLifecycleDebugSnapshot {
        pub before_sync_runtime_output_windows: usize,
        pub after_sync_runtime_output_windows: usize,
        pub before_pending_discovery_windows: usize,
        pub after_pending_discovery_windows: usize,
        pub before_controller_apply_command_windows: usize,
        pub after_controller_apply_command_windows: usize,
        pub before_capture_resize_snapshots_windows: usize,
        pub after_capture_resize_snapshots_windows: usize,
        pub after_refresh_workspace_export_windows: usize,
        pub before_apply_pending_workspace_actions_windows: usize,
        pub after_apply_pending_workspace_actions_windows: usize,
        pub before_sync_focus_state_windows: usize,
        pub after_sync_focus_state_windows: usize,
        pub before_pending_discovery_generation: u64,
        pub after_controller_apply_command_generation: u64,
        pub before_apply_pending_workspace_actions_generation: u64,
        pub recent_events: Vec<String>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WindowPresentationDebugSnapshot {
        pub window_id: WindowId,
        pub target_size: Option<(i32, i32)>,
        pub acked_size: Option<(i32, i32)>,
        pub committed_view_size: Option<(i32, i32)>,
        pub has_been_presented: bool,
        pub has_resize_snapshot: bool,
        pub snapshot_captured_for_transition: bool,
        pub used_snapshot_on_last_frame: bool,
        pub used_live_content_on_last_frame: bool,
        pub newly_presented_on_last_frame: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct SmithayBootstrapTopologySnapshot {
        pub active_output_id: Option<OutputId>,
        pub active_seat_name: Option<String>,
        pub outputs: Vec<OutputState>,
        pub seats: Vec<SeatState>,
        pub surfaces: Vec<SurfaceState>,
    }

    #[derive(Debug)]
    pub struct SmithayBootstrap<R> {
        pub controller: crate::CompositorController<R>,
        pub runtime: SmithayWinitRuntime<'static>,
        pub report: SmithayStartupReport,
        pub lifecycle_debug: SmithayLifecycleDebugSnapshot,
    }

    impl<R> SmithayBootstrap<R>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        pub fn run_startup_cycle(&mut self) -> Result<(), SmithayRuntimeError> {
            self.runtime.run_startup_cycle()?;
            self.lifecycle_debug.before_sync_runtime_output_windows =
                self.controller.state_snapshot().windows.len();
            self.sync_runtime_output_size()?;
            self.lifecycle_debug.after_sync_runtime_output_windows =
                self.controller.state_snapshot().windows.len();

            self.lifecycle_debug.before_pending_discovery_windows =
                self.controller.state_snapshot().windows.len();
            self.apply_pending_discovery_events()?;
            self.lifecycle_debug.after_pending_discovery_windows =
                self.controller.state_snapshot().windows.len();
            Ok(())
        }

        fn sync_runtime_output_size(&mut self) -> Result<(), SmithayRuntimeError> {
            let window_size = self.runtime.window_size;
            let controller_state = self.controller.state_snapshot();
            let output_id = controller_state
                .current_output_id
                .clone()
                .or_else(|| {
                    controller_state
                        .outputs
                        .first()
                        .map(|output| output.id.clone())
                })
                .or_else(|| {
                    self.runtime
                        .state()
                        .snapshot()
                        .outputs
                        .active_output_id
                        .clone()
                })
                .unwrap_or_else(|| OutputId::from("smithay-winit-output"));

            let current_output = controller_state
                .outputs
                .iter()
                .find(|output| output.id == output_id)
                .cloned();

            let width = window_size.0.max(0) as u32;
            let height = window_size.1.max(0) as u32;

            if current_output.as_ref().is_some_and(|output| {
                output.logical_width == width && output.logical_height == height
            }) {
                return Ok(());
            }

            self.apply_controller_command(ControllerCommand::DiscoveryEvent(
                crate::backend::BackendDiscoveryEvent::OutputSnapshotDiscovered {
                    output: OutputSnapshot {
                        id: output_id.clone(),
                        name: current_output
                            .as_ref()
                            .map(|output| output.name.clone())
                            .unwrap_or_else(|| output_id.to_string()),
                        logical_x: current_output
                            .as_ref()
                            .map(|output| output.logical_x)
                            .unwrap_or(0),
                        logical_y: current_output
                            .as_ref()
                            .map(|output| output.logical_y)
                            .unwrap_or(0),
                        logical_width: width,
                        logical_height: height,
                        scale: current_output
                            .as_ref()
                            .map(|output| output.scale)
                            .unwrap_or(1),
                        transform: current_output
                            .as_ref()
                            .map(|output| output.transform)
                            .unwrap_or(spiders_shared::wm::OutputTransform::Normal),
                        enabled: current_output
                            .as_ref()
                            .map(|output| output.enabled)
                            .unwrap_or(true),
                        current_workspace_id: current_output
                            .as_ref()
                            .and_then(|output| output.current_workspace_id.clone()),
                    },
                    active: true,
                },
            ))
        }

        pub fn run_until_exit(&mut self) -> Result<(), SmithayRuntimeError> {
            while !self.runtime.should_stop() {
                self.run_startup_cycle()?;
                std::thread::sleep(Duration::from_millis(16));
            }
            Ok(())
        }

        pub fn snapshot(&self) -> SmithayBootstrapSnapshot {
            let topology = self.controller.app().topology();
            SmithayBootstrapSnapshot {
                runtime: self.runtime.snapshot(),
                controller: self.controller.report(),
                topology: snapshot_topology(topology),
                runtime_bootstrap_debug: self.controller.app().runtime_bootstrap_debug.clone(),
                lifecycle_debug: self.lifecycle_debug.clone(),
                topology_surface_count: topology.surfaces.len(),
                topology_output_count: topology.outputs.len(),
                topology_seat_count: topology.seats.len(),
            }
        }

        pub fn apply_pending_discovery_events(&mut self) -> Result<usize, SmithayRuntimeError> {
            let commands = self.runtime.drain_pending_discovery_commands();
            self.lifecycle_debug.before_pending_discovery_generation =
                self.controller.app().session().debug_generation();
            append_winit_debug_log(&format!(
                "bootstrap.apply_pending_discovery_events start commands={} windows={} visible={} gen={}",
                commands.len(),
                self.controller.state_snapshot().windows.len(),
                self.controller.state_snapshot().visible_window_ids.len(),
                self.controller.app().session().debug_generation()
            ));
            let applied = self.apply_pending_discovery_commands(commands)?;
            self.lifecycle_debug
                .before_apply_pending_workspace_actions_windows =
                self.controller.state_snapshot().windows.len();
            self.lifecycle_debug
                .before_apply_pending_workspace_actions_generation =
                self.controller.app().session().debug_generation();
            append_winit_debug_log(&format!(
                "bootstrap.before_apply_pending_workspace_actions windows={} visible={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.state_snapshot().visible_window_ids.len(),
                self.controller.app().session().debug_generation()
            ));
            self.apply_pending_workspace_actions()?;
            self.lifecycle_debug
                .after_apply_pending_workspace_actions_windows =
                self.controller.state_snapshot().windows.len();
            append_winit_debug_log(&format!(
                "bootstrap.after_apply_pending_workspace_actions windows={} visible={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.state_snapshot().visible_window_ids.len(),
                self.controller.app().session().debug_generation()
            ));
            self.lifecycle_debug.before_sync_focus_state_windows =
                self.controller.state_snapshot().windows.len();
            append_winit_debug_log(&format!(
                "bootstrap.before_sync_focus_state windows={} visible={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.state_snapshot().visible_window_ids.len(),
                self.controller.app().session().debug_generation()
            ));
            self.sync_focus_state()?;
            self.lifecycle_debug.after_sync_focus_state_windows =
                self.controller.state_snapshot().windows.len();
            append_winit_debug_log(&format!(
                "bootstrap.after_sync_focus_state windows={} visible={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.state_snapshot().visible_window_ids.len(),
                self.controller.app().session().debug_generation()
            ));
            Ok(applied)
        }

        pub fn apply_pending_workspace_actions(&mut self) -> Result<usize, SmithayRuntimeError> {
            let actions = self.runtime.take_workspace_actions();
            let mut applied = 0;

            append_winit_debug_log(&format!(
                "bootstrap.apply_pending_workspace_actions drained={} windows={} visible={} focused={:?} workspace={:?}",
                actions.len(),
                self.controller.state_snapshot().windows.len(),
                self.controller.state_snapshot().visible_window_ids.len(),
                self.controller.state_snapshot().focused_window_id,
                self.controller.state_snapshot().current_workspace_id,
            ));

            for action in actions {
                append_winit_debug_log(&format!(
                    "bootstrap.apply_pending_workspace_actions action_start action={action:?} windows={} visible={} focused={:?} workspace={:?} gen={}",
                    self.controller.state_snapshot().windows.len(),
                    self.controller.state_snapshot().visible_window_ids.len(),
                    self.controller.state_snapshot().focused_window_id,
                    self.controller.state_snapshot().current_workspace_id,
                    self.controller.app().session().debug_generation(),
                ));
                if let WmAction::Spawn { command } = &action {
                    spawn_shell_command(command, self.runtime.socket_name())?;
                    append_winit_debug_log(&format!(
                        "bootstrap.apply_pending_workspace_actions action_end action={action:?} windows={} visible={} focused={:?} workspace={:?} gen={}",
                        self.controller.state_snapshot().windows.len(),
                        self.controller.state_snapshot().visible_window_ids.len(),
                        self.controller.state_snapshot().focused_window_id,
                        self.controller.state_snapshot().current_workspace_id,
                        self.controller.app().session().debug_generation(),
                    ));
                    applied += 1;
                    continue;
                }
                if let WmAction::FocusWindow { window_id } = &action {
                    let window_known = self
                        .controller
                        .state_snapshot()
                        .windows
                        .iter()
                        .any(|window| window.id == *window_id);

                    if !window_known {
                        append_winit_debug_log(&format!(
                            "bootstrap.apply_pending_workspace_actions skip_unknown_focus window_id={window_id:?}"
                        ));
                        applied += 1;
                        continue;
                    }
                }
                if action == WmAction::CloseFocusedWindow {
                    let focused_window_id = self
                        .controller
                        .state_snapshot()
                        .focused_window_id
                        .clone()
                        .or_else(|| {
                            self.controller
                                .state_snapshot()
                                .visible_window_ids
                                .first()
                                .cloned()
                        });

                    if let Some(window_id) = focused_window_id.as_ref() {
                        let previous_window_plan =
                            self.runtime.state().current_window_render_plan().to_vec();
                        let previous_titlebars =
                            self.runtime.state().current_titlebar_render_plan().to_vec();
                        let decoration_policies = self
                            .runtime
                            .state()
                            .current_window_decoration_policies()
                            .to_vec();
                        if let Some(render) = previous_window_plan
                            .iter()
                            .find(|render| &render.window_id == window_id)
                            .cloned()
                        {
                            let surfaces = self.runtime.state().renderable_toplevel_surfaces();
                            if let Some(surface) = surfaces
                                .iter()
                                .find(|surface| &surface.window_id == window_id)
                                .cloned()
                            {
                                if let Some(output) = self.runtime.state().active_smithay_output() {
                                    let output_scale = smithay::utils::Scale::from(
                                        output.current_scale().fractional_scale(),
                                    );
                                    let mut request_redraw = false;
                                    if let Some(backend) = self.runtime.backend.as_mut() {
                                        if let Ok((renderer, _)) = backend.bind() {
                                            if let Ok(Some(snapshot)) = capture_window_snapshot(
                                                renderer,
                                                &surface,
                                                &render,
                                                output_scale,
                                            ) {
                                                if let Ok(Some(scene_snapshot)) =
                                                    capture_scene_snapshot(
                                                        renderer,
                                                        &previous_window_plan,
                                                        &previous_titlebars,
                                                        &decoration_policies,
                                                        &surfaces,
                                                        self.runtime.window_size.into(),
                                                        output_scale,
                                                    )
                                                {
                                                    let affected_windows =
                                                        projected_close_affected_window_ids(
                                                            &previous_window_plan,
                                                            window_id,
                                                        );
                                                    let affected_previous_windows =
                                                        select_windows_by_ids(
                                                            &previous_window_plan,
                                                            &affected_windows,
                                                        );
                                                    let next_transition = SceneTransition::new(
                                                        scene_snapshot,
                                                        transition_now(
                                                            &self.runtime.presentation_state.clock,
                                                        ),
                                                        layout_signature(&previous_window_plan),
                                                        affected_windows,
                                                        union_window_rect(
                                                            &affected_previous_windows,
                                                        )
                                                        .unwrap_or(render.window_rect),
                                                    );
                                                    self.runtime.scene_transition =
                                                        Some(merge_scene_transition(
                                                            self.runtime.scene_transition.take(),
                                                            next_transition,
                                                        ));
                                                }
                                                self.runtime
                                                    .last_snapshot_capture_window_ids
                                                    .insert(window_id.clone());
                                                self.runtime.closing_transitions.insert(
                                                    window_id.clone(),
                                                    ClosingWindowTransition::new(
                                                        window_id.clone(),
                                                        snapshot,
                                                        transition_now(
                                                            &self.runtime.presentation_state.clock,
                                                        ),
                                                    ),
                                                );
                                                self.runtime.resize_transitions.insert(
                                                    window_id.clone(),
                                                    ResizeTransition::new(
                                                        window_id.clone(),
                                                        self.runtime
                                                            .closing_transitions
                                                            .get(window_id)
                                                            .expect("closing transition inserted")
                                                            .snapshot
                                                            .clone(),
                                                        (
                                                            render
                                                                .window_rect
                                                                .width
                                                                .max(0.0)
                                                                .round()
                                                                as i32,
                                                            (render.window_rect.height
                                                                - render.content_offset_y)
                                                                .max(0.0)
                                                                .round()
                                                                as i32,
                                                        ),
                                                        transition_now(
                                                            &self.runtime.presentation_state.clock,
                                                        ),
                                                        true,
                                                    ),
                                                );
                                                request_redraw = true;
                                            }
                                        }
                                    }
                                    if request_redraw {
                                        self.runtime.state_mut().request_redraw();
                                    }
                                }
                            }
                        }
                    }

                    let close_requested = focused_window_id.as_ref().is_some_and(|window_id| {
                        self.runtime.state_mut().request_close_for_window(window_id)
                    });

                    append_winit_debug_log(&format!(
                        "bootstrap.apply_pending_workspace_actions close_requested focused={focused_window_id:?} requested={close_requested}"
                    ));

                    if close_requested {
                        append_winit_debug_log(&format!(
                            "bootstrap.apply_pending_workspace_actions action_end action={action:?} windows={} visible={} focused={:?} workspace={:?} gen={}",
                            self.controller.state_snapshot().windows.len(),
                            self.controller.state_snapshot().visible_window_ids.len(),
                            self.controller.state_snapshot().focused_window_id,
                            self.controller.state_snapshot().current_workspace_id,
                            self.controller.app().session().debug_generation(),
                        ));
                        applied += 1;
                        continue;
                    }
                }
                let previous_window_plan =
                    self.runtime.state().current_window_render_plan().to_vec();
                let previous_presented_window_ids = self
                    .runtime
                    .state()
                    .snapshot()
                    .known_surfaces
                    .toplevels
                    .iter()
                    .filter(|surface| surface.has_been_presented)
                    .map(|surface| surface.window_id.clone())
                    .collect::<std::collections::HashSet<_>>();

                self.controller.apply_ipc_action(&action).map_err(|error| {
                    SmithayRuntimeError::Winit(format!("workspace action failed: {error}"))
                })?;
                self.capture_resize_snapshots_for_next_export(
                    &previous_window_plan,
                    &previous_presented_window_ids,
                )?;
                refresh_workspace_export_from_controller(
                    &self.controller,
                    self.runtime.state_mut(),
                );
                self.report.controller = self.controller.report();
                append_winit_debug_log(&format!(
                    "bootstrap.apply_pending_workspace_actions action_end action={action:?} windows={} visible={} focused={:?} workspace={:?} gen={}",
                    self.controller.state_snapshot().windows.len(),
                    self.controller.state_snapshot().visible_window_ids.len(),
                    self.controller.state_snapshot().focused_window_id,
                    self.controller.state_snapshot().current_workspace_id,
                    self.controller.app().session().debug_generation(),
                ));
                applied += 1;
            }

            Ok(applied)
        }

        fn sync_focus_state(&mut self) -> Result<(), SmithayRuntimeError> {
            let controller_state = self.controller.state_snapshot();
            let focused_window_id = controller_state
                .focused_window_id
                .clone()
                .or_else(|| controller_state.visible_window_ids.first().cloned());

            let Some(window_id) = focused_window_id else {
                return Ok(());
            };

            let window_known = controller_state
                .windows
                .iter()
                .any(|window| window.id == window_id);

            if !window_known {
                append_winit_debug_log(&format!(
                    "bootstrap.sync_focus_state skip_unknown_window window_id={window_id:?} visible={:?} focused={:?}",
                    controller_state.visible_window_ids, controller_state.focused_window_id,
                ));
                return Ok(());
            }

            if controller_state.focused_window_id.is_none() {
                self.controller
                    .apply_ipc_action(&WmAction::FocusWindow {
                        window_id: window_id.clone(),
                    })
                    .map_err(|error| {
                        SmithayRuntimeError::Winit(format!("workspace action failed: {error}"))
                    })?;
                refresh_workspace_export_from_controller(
                    &self.controller,
                    self.runtime.state_mut(),
                );
                self.report.controller = self.controller.report();
            }

            let runtime_focus = self
                .runtime
                .state()
                .snapshot()
                .seat
                .focused_window_id
                .clone();
            if runtime_focus.as_ref() != Some(&window_id) {
                self.runtime
                    .state_mut()
                    .set_keyboard_focus_for_window(&window_id);
            }

            Ok(())
        }

        pub fn apply_adapter_event(
            &mut self,
            event: SmithayAdapterEvent,
        ) -> Result<(), SmithayRuntimeError> {
            self.apply_controller_command(SmithayAdapter::translate_event(event))
        }

        pub fn apply_adapter_events(
            &mut self,
            events: Vec<SmithayAdapterEvent>,
        ) -> Result<usize, SmithayRuntimeError> {
            let commands = events
                .into_iter()
                .map(SmithayAdapter::translate_event)
                .collect();
            self.apply_pending_discovery_commands(commands)
        }

        pub fn apply_adapter_surface_discovery_batch(
            &mut self,
            generation: u64,
            surfaces: Vec<crate::backend::BackendSurfaceSnapshot>,
        ) -> Result<(), SmithayRuntimeError> {
            self.apply_controller_command(SmithayAdapter::translate_snapshot(
                generation,
                Vec::new(),
                Vec::new(),
                surfaces,
            ))
        }

        pub fn apply_tracked_smithay_surface_discovery(
            &mut self,
            generation: u64,
        ) -> Result<(), SmithayRuntimeError> {
            let surfaces = self.runtime.state().backend_surface_snapshots();
            self.apply_adapter_surface_discovery_batch(generation, surfaces)
        }

        pub fn apply_tracked_smithay_discovery_snapshot(
            &mut self,
            generation: u64,
        ) -> Result<(), SmithayRuntimeError> {
            let snapshot = self.runtime.state().backend_topology_snapshot(generation);
            self.apply_controller_command(ControllerCommand::DiscoverySnapshot(snapshot))
        }

        pub fn apply_adapter_discovery_batch(
            &mut self,
            generation: u64,
            seats: Vec<crate::backend::BackendSeatSnapshot>,
            outputs: Vec<crate::backend::BackendOutputSnapshot>,
            surfaces: Vec<crate::backend::BackendSurfaceSnapshot>,
        ) -> Result<(), SmithayRuntimeError> {
            self.apply_controller_command(SmithayAdapter::translate_snapshot(
                generation, seats, outputs, surfaces,
            ))
        }

        pub fn apply_pending_discovery_commands(
            &mut self,
            commands: Vec<ControllerCommand>,
        ) -> Result<usize, SmithayRuntimeError> {
            let mut applied = 0;

            for command in commands {
                self.apply_controller_command(command)?;
                applied += 1;
            }

            Ok(applied)
        }

        fn apply_controller_command(
            &mut self,
            command: ControllerCommand,
        ) -> Result<(), SmithayRuntimeError> {
            self.lifecycle_debug.recent_events.push(format!(
                "before {command:?} windows={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.app().session().debug_generation()
            ));
            if self.lifecycle_debug.recent_events.len() > 32 {
                self.lifecycle_debug.recent_events.remove(0);
            }
            self.lifecycle_debug.before_controller_apply_command_windows =
                self.controller.state_snapshot().windows.len();
            let previous_window_plan = self.runtime.state().current_window_render_plan().to_vec();
            let previous_presented_window_ids = self
                .runtime
                .state()
                .snapshot()
                .known_surfaces
                .toplevels
                .iter()
                .filter(|surface| surface.has_been_presented)
                .map(|surface| surface.window_id.clone())
                .collect::<std::collections::HashSet<_>>();
            self.controller.apply_command(command)?;
            self.lifecycle_debug.after_controller_apply_command_windows =
                self.controller.state_snapshot().windows.len();
            self.lifecycle_debug
                .after_controller_apply_command_generation =
                self.controller.app().session().debug_generation();
            self.lifecycle_debug.recent_events.push(format!(
                "after controller.apply_command windows={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.app().session().debug_generation()
            ));
            if self.lifecycle_debug.recent_events.len() > 32 {
                self.lifecycle_debug.recent_events.remove(0);
            }
            self.lifecycle_debug.before_capture_resize_snapshots_windows =
                self.controller.state_snapshot().windows.len();
            self.capture_resize_snapshots_for_next_export(
                &previous_window_plan,
                &previous_presented_window_ids,
            )?;
            self.lifecycle_debug.after_capture_resize_snapshots_windows =
                self.controller.state_snapshot().windows.len();
            self.lifecycle_debug.recent_events.push(format!(
                "after capture_resize_snapshots windows={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.app().session().debug_generation()
            ));
            if self.lifecycle_debug.recent_events.len() > 32 {
                self.lifecycle_debug.recent_events.remove(0);
            }
            refresh_workspace_export_from_controller(&self.controller, self.runtime.state_mut());
            self.lifecycle_debug.after_refresh_workspace_export_windows =
                self.controller.state_snapshot().windows.len();
            self.lifecycle_debug.recent_events.push(format!(
                "after refresh_workspace_export windows={} gen={}",
                self.controller.state_snapshot().windows.len(),
                self.controller.app().session().debug_generation()
            ));
            if self.lifecycle_debug.recent_events.len() > 32 {
                self.lifecycle_debug.recent_events.remove(0);
            }
            self.report.controller = self.controller.report();
            Ok(())
        }

        fn capture_resize_snapshots_for_next_export(
            &mut self,
            previous_window_plan: &[SmithayWindowRenderSnapshot],
            previously_presented_window_ids: &std::collections::HashSet<WindowId>,
        ) -> Result<(), SmithayRuntimeError> {
            self.runtime.last_snapshot_capture_window_ids.clear();
            let previous_titlebars = self.runtime.state().current_titlebar_render_plan().to_vec();
            let next_window_placements =
                self.controller.app().session().current_window_placements();
            let next_titlebars = self
                .controller
                .app()
                .session()
                .current_titlebar_render_plan();
            let decoration_policies = self.runtime.state().current_window_decoration_policies();
            let next_window_plan =
                build_window_render_plan(&next_window_placements, &next_titlebars);
            let affected_windows =
                affected_window_ids_for_transition(previous_window_plan, &next_window_plan);
            let affected_window_ids = affected_windows.iter().cloned().collect::<HashSet<_>>();
            let next_window_ids = next_window_plan
                .iter()
                .map(|window| window.window_id.clone())
                .collect::<HashSet<_>>();

            let state = self.runtime.state_mut();
            state.preview_window_render_plan_configures(&next_window_plan);

            let surfaces = state.renderable_toplevel_surfaces();
            let Some(output) = state.active_smithay_output() else {
                return Ok(());
            };
            let output_scale =
                smithay::utils::Scale::from(output.current_scale().fractional_scale());
            let Some(backend) = self.runtime.backend.as_mut() else {
                return Ok(());
            };
            let (renderer, _) = backend
                .bind()
                .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
            let mut request_redraw = false;
            let mut captured_scene_transition = false;

            if !affected_windows.is_empty() {
                if let Some(scene_snapshot) = capture_scene_snapshot(
                    renderer,
                    previous_window_plan,
                    &previous_titlebars,
                    &decoration_policies,
                    &surfaces,
                    self.runtime.window_size.into(),
                    output_scale,
                )? {
                    let affected_previous_windows =
                        select_windows_by_ids(previous_window_plan, &affected_windows);
                    let frozen_rect =
                        union_window_rect(&affected_previous_windows).unwrap_or(LayoutRect {
                            x: 0.0,
                            y: 0.0,
                            width: self.runtime.window_size.0 as f32,
                            height: self.runtime.window_size.1 as f32,
                        });
                    let next_transition = SceneTransition::new(
                        scene_snapshot,
                        transition_now(&self.runtime.presentation_state.clock),
                        layout_signature(previous_window_plan),
                        affected_windows.clone(),
                        frozen_rect,
                    );
                    self.runtime.scene_transition = Some(merge_scene_transition(
                        self.runtime.scene_transition.take(),
                        next_transition,
                    ));
                    self.runtime
                        .resize_transitions
                        .retain(|window_id, _| !affected_window_ids.contains(window_id));
                    self.runtime.closing_transitions.retain(|window_id, _| {
                        !affected_window_ids.contains(window_id)
                            || next_window_ids.contains(window_id)
                    });
                    captured_scene_transition = true;
                    request_redraw = true;
                }
            }

            for previous in previous_window_plan {
                if !previously_presented_window_ids.contains(&previous.window_id) {
                    continue;
                }

                if captured_scene_transition && affected_window_ids.contains(&previous.window_id) {
                    continue;
                }

                let Some(next) = next_window_plan
                    .iter()
                    .find(|window| window.window_id == previous.window_id)
                else {
                    continue;
                };

                if previous == next {
                    let should_keep_existing_transition = self
                        .runtime
                        .resize_transitions
                        .get(&previous.window_id)
                        .is_some_and(|transition| {
                            !resize_transition_can_release(
                                Some(transition.target_logical_size),
                                transition,
                                transition_now(&self.runtime.presentation_state.clock),
                            )
                        });
                    if !should_keep_existing_transition {
                        self.runtime.resize_transitions.remove(&previous.window_id);
                    }
                    continue;
                }

                let Some(surface) = surfaces
                    .iter()
                    .find(|surface| surface.window_id == previous.window_id)
                else {
                    continue;
                };

                if let Some(snapshot) =
                    capture_window_snapshot(renderer, surface, previous, output_scale)?
                {
                    let _ = snapshot;
                    request_redraw = true;
                }
            }

            let _ = renderer;
            let _ = backend;
            if request_redraw {
                self.runtime.state_mut().request_redraw();
            }

            Ok(())
        }
    }

    #[derive(Debug)]
    pub struct SmithayWinitRuntime<'a> {
        event_loop: EventLoop<'a, SpidersSmithayState>,
        display_handle: smithay::reexports::wayland_server::DisplayHandle,
        loop_signal: LoopSignal,
        socket_name: String,
        window_size: (i32, i32),
        state: Option<SpidersSmithayState>,
        render_state: Option<TitlebarRenderState>,
        presentation_state: PresentationRenderState,
        scene_transition: Option<SceneTransition>,
        resize_transitions: HashMap<WindowId, ResizeTransition>,
        closing_transitions: HashMap<WindowId, ClosingWindowTransition>,
        opening_transitions: HashMap<WindowId, OpeningWindowTransition>,
        pending_presented_window_ids: HashSet<WindowId>,
        frame_debug_history: VecDeque<FrameDebugRecord>,
        next_frame_debug_index: u64,
        last_snapshot_capture_window_ids: HashSet<WindowId>,
        last_snapshot_used_window_ids: HashSet<WindowId>,
        last_live_window_ids: HashSet<WindowId>,
        last_newly_presented_window_ids: HashSet<WindowId>,
        backend: Option<WinitGraphicsBackend<GlesRenderer>>,
        winit: Option<WinitEventLoop>,
        stopped: bool,
    }

    impl SmithayWinitRuntime<'_> {
        fn push_frame_debug(&mut self, record: FrameDebugRecord) {
            self.frame_debug_history.push_back(record);
            while self.frame_debug_history.len() > FRAME_DEBUG_HISTORY_LIMIT {
                self.frame_debug_history.pop_front();
            }
        }

        fn record_frame_debug(
            &mut self,
            reason: &str,
            rendered: bool,
            windows: &[SmithayWindowRenderSnapshot],
            surfaces: &[SmithayRenderableToplevelSurface],
            presented_window_ids: &HashSet<WindowId>,
            pending_presented_window_ids: &HashSet<WindowId>,
        ) {
            let transition_now_value = transition_now(&self.presentation_state.clock);
            let record = FrameDebugRecord {
                frame_index: self.next_frame_debug_index,
                rendered,
                reason: reason.into(),
                focused_window_id: self.state().snapshot().seat.focused_window_id,
                presented_window_ids: presented_window_ids.iter().cloned().collect(),
                pending_presented_window_ids: pending_presented_window_ids
                    .iter()
                    .cloned()
                    .collect(),
                scene_transaction: scene_transaction_debug_record(
                    self.scene_transition.as_ref(),
                    windows,
                    surfaces,
                    &pending_presented_window_ids,
                    transition_now_value,
                    false,
                ),
                windows: windows
                    .iter()
                    .map(|window| {
                        let surface = surfaces
                            .iter()
                            .find(|surface| surface.window_id == window.window_id);
                        let snapshot = self
                            .resize_transitions
                            .get(&window.window_id)
                            .map(|transition| &transition.from)
                            .or_else(|| {
                                self.closing_transitions
                                    .get(&window.window_id)
                                    .map(|transition| &transition.snapshot)
                            });
                        FrameWindowDebugRecord {
                            window_id: window.window_id.clone(),
                            target_rect: (
                                window.window_rect.x.round() as i32,
                                window.window_rect.y.round() as i32,
                                window.window_rect.width.round() as i32,
                                window.window_rect.height.round() as i32,
                            ),
                            committed_size: surface.and_then(|surface| surface.committed_size),
                            committed_view_size: surface.and_then(|surface| {
                                committed_root_surface_size(surface.surface.wl_surface())
                            }),
                            has_buffer: surface.is_some_and(|surface| surface.has_buffer),
                            has_resize_snapshot: snapshot.is_some(),
                            snapshot_rect: snapshot.map(|snapshot| {
                                (
                                    snapshot.render_snapshot.window_rect.x.round() as i32,
                                    snapshot.render_snapshot.window_rect.y.round() as i32,
                                    snapshot.render_snapshot.window_rect.width.round() as i32,
                                    snapshot.render_snapshot.window_rect.height.round() as i32,
                                )
                            }),
                            considered_presented: presented_window_ids.contains(&window.window_id),
                            pending_presented: pending_presented_window_ids
                                .contains(&window.window_id),
                            drew_live: false,
                            drew_snapshot: false,
                        }
                    })
                    .collect(),
            };
            self.push_frame_debug(record);
            self.next_frame_debug_index += 1;
        }

        fn write_frame_debug_log(&self) {
            let Some(path) = std::env::var_os("SPIDERS_WM_WINIT_DEBUG_FRAME_PATH") else {
                return;
            };

            let mut body = String::new();
            for frame in &self.frame_debug_history {
                body.push_str(&format!(
                    "frame={} rendered={} reason={} focused={:?} presented={:?} pending={:?}\n",
                    frame.frame_index,
                    frame.rendered,
                    frame.reason,
                    frame.focused_window_id,
                    frame.presented_window_ids,
                    frame.pending_presented_window_ids,
                ));
                if let Some(transaction) = &frame.scene_transaction {
                    body.push_str(&format!(
                        "  scene affected={:?} frozen_rect={:?} alpha_milli={} layout_changed={} settled={} emitted_overlay={}\n",
                        transaction.affected_windows,
                        transaction.frozen_rect,
                        transaction.alpha_milli,
                        transaction.layout_changed,
                        transaction.settled,
                        transaction.emitted_overlay,
                    ));
                }
                for window in &frame.windows {
                    body.push_str(&format!(
                        "  window={:?} target={:?} committed={:?} view={:?} buffer={} snapshot={} snapshot_rect={:?} considered_presented={} pending={} drew_live={} drew_snapshot={}\n",
                        window.window_id,
                        window.target_rect,
                        window.committed_size,
                        window.committed_view_size,
                        window.has_buffer,
                        window.has_resize_snapshot,
                        window.snapshot_rect,
                        window.considered_presented,
                        window.pending_presented,
                        window.drew_live,
                        window.drew_snapshot,
                    ));
                }
            }

            let _ = std::fs::write(path, body);
        }

        pub fn socket_name(&self) -> &str {
            &self.socket_name
        }

        pub fn display_handle(&self) -> &smithay::reexports::wayland_server::DisplayHandle {
            &self.display_handle
        }

        pub fn loop_signal(&self) -> &LoopSignal {
            &self.loop_signal
        }

        pub fn state(&self) -> &SpidersSmithayState {
            self.state.as_ref().expect("smithay runtime state missing")
        }

        pub fn state_mut(&mut self) -> &mut SpidersSmithayState {
            self.state.as_mut().expect("smithay runtime state missing")
        }

        pub fn snapshot(&self) -> SmithayRuntimeSnapshot {
            let state_snapshot = self.state().snapshot();
            let window_plan = self.state().current_window_render_plan().to_vec();
            let surfaces = self.state().renderable_toplevel_surfaces();
            SmithayRuntimeSnapshot {
                socket_name: self.socket_name.clone(),
                window_size: self.window_size,
                presentation_debug: window_plan
                    .iter()
                    .map(|window| {
                        let surface = surfaces
                            .iter()
                            .find(|surface| surface.window_id == window.window_id);
                        WindowPresentationDebugSnapshot {
                            window_id: window.window_id.clone(),
                            target_size: Some((
                                window.window_rect.width.max(0.0).round() as i32,
                                (window.window_rect.height - window.content_offset_y)
                                    .max(0.0)
                                    .round() as i32,
                            )),
                            acked_size: surface.and_then(|surface| surface.committed_size),
                            committed_view_size: surface.and_then(|surface| {
                                committed_root_surface_size(surface.surface.wl_surface())
                            }),
                            has_been_presented: state_snapshot.known_surfaces.toplevels.iter().any(
                                |known| {
                                    known.window_id == window.window_id && known.has_been_presented
                                },
                            ),
                            has_resize_snapshot: self
                                .resize_transitions
                                .contains_key(&window.window_id)
                                || self.closing_transitions.contains_key(&window.window_id),
                            snapshot_captured_for_transition: self
                                .last_snapshot_capture_window_ids
                                .contains(&window.window_id),
                            used_snapshot_on_last_frame: self
                                .last_snapshot_used_window_ids
                                .contains(&window.window_id),
                            used_live_content_on_last_frame: self
                                .last_live_window_ids
                                .contains(&window.window_id),
                            newly_presented_on_last_frame: self
                                .last_newly_presented_window_ids
                                .contains(&window.window_id),
                        }
                    })
                    .collect(),
                frame_debug_history: self
                    .frame_debug_history
                    .iter()
                    .cloned()
                    .map(|record| FrameDebugSnapshot {
                        frame_index: record.frame_index,
                        rendered: record.rendered,
                        reason: record.reason,
                        focused_window_id: record.focused_window_id,
                        presented_window_ids: record.presented_window_ids,
                        pending_presented_window_ids: record.pending_presented_window_ids,
                        scene_transaction: record.scene_transaction.map(|transaction| {
                            FrameSceneTransactionDebugSnapshot {
                                affected_windows: transaction.affected_windows,
                                frozen_rect: transaction.frozen_rect,
                                alpha_milli: transaction.alpha_milli,
                                layout_changed: transaction.layout_changed,
                                settled: transaction.settled,
                                emitted_overlay: transaction.emitted_overlay,
                            }
                        }),
                        windows: record
                            .windows
                            .into_iter()
                            .map(|window| FrameWindowDebugSnapshot {
                                window_id: window.window_id,
                                target_rect: window.target_rect,
                                committed_size: window.committed_size,
                                committed_view_size: window.committed_view_size,
                                has_buffer: window.has_buffer,
                                has_resize_snapshot: window.has_resize_snapshot,
                                snapshot_rect: window.snapshot_rect,
                                considered_presented: window.considered_presented,
                                pending_presented: window.pending_presented,
                                drew_live: window.drew_live,
                                drew_snapshot: window.drew_snapshot,
                            })
                            .collect(),
                    })
                    .collect(),
                state: state_snapshot,
            }
        }

        pub fn should_stop(&self) -> bool {
            self.stopped
        }

        pub fn run_startup_cycle(&mut self) -> Result<(), SmithayRuntimeError> {
            self.dispatch_winit_events()?;

            self.render_if_needed()?;

            let state = self.state.as_mut().ok_or_else(|| {
                SmithayRuntimeError::Winit("smithay runtime state missing".into())
            })?;

            self.event_loop
                .dispatch(Some(Duration::ZERO), state)
                .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

            state
                .display_handle
                .flush_clients()
                .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

            Ok(())
        }

        fn apply_winit_cursor_feedback(&mut self) {
            let cursor_image = self.state().snapshot().seat.cursor_image;
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            if cursor_image == "hidden" {
                backend.window().set_cursor_visible(false);
                return;
            }

            backend.window().set_cursor_visible(true);
            backend
                .window()
                .set_cursor(cursor_icon_for_snapshot(&cursor_image));
        }

        fn render_if_needed(&mut self) -> Result<(), SmithayRuntimeError> {
            let (
                should_render,
                render_items,
                window_items,
                decoration_policies,
                output,
                surfaces,
                presented_window_ids,
                newly_presented_window_ids,
                unseen_window_ids,
                dropped_window_ids,
                current_window_ids,
            ) = {
                let pending_presented_window_ids = self.pending_presented_window_ids.clone();
                let state = self.state_mut();
                let should_render = state.take_redraw_request();
                let render_items = state.current_titlebar_render_plan().to_vec();
                let window_items = state.current_window_render_plan().to_vec();
                let decoration_policies = state.current_window_decoration_policies();
                let output = state.active_smithay_output();
                let surfaces = state.renderable_toplevel_surfaces();
                let unseen_window_ids = window_items
                    .iter()
                    .filter(|window| {
                        !state.has_been_presented(&window.window_id)
                            && !pending_presented_window_ids.contains(&window.window_id)
                    })
                    .map(|window| window.window_id.clone())
                    .collect::<Vec<_>>();
                let pending_new_window_ids = window_items
                    .iter()
                    .filter_map(|window| {
                        if state.has_been_presented(&window.window_id)
                            || pending_presented_window_ids.contains(&window.window_id)
                        {
                            return None;
                        }
                        let expected_width = window.window_rect.width.max(0.0).round() as i32;
                        let expected_height = (window.window_rect.height - window.content_offset_y)
                            .max(0.0)
                            .round() as i32;
                        surfaces
                            .iter()
                            .find(|surface| surface.window_id == window.window_id)
                            .and_then(|surface| {
                                let committed =
                                    committed_root_surface_size(surface.surface.wl_surface())
                                        .or(surface.committed_size);
                                ready_for_initial_presentation(
                                    committed,
                                    surface.has_buffer,
                                    (expected_width, expected_height),
                                )
                                .then(|| window.window_id.clone())
                            })
                    })
                    .collect::<Vec<_>>();

                let current_window_ids = window_items
                    .iter()
                    .map(|window| window.window_id.clone())
                    .collect::<std::collections::HashSet<_>>();
                let dropped_window_ids = state
                    .snapshot()
                    .known_surfaces
                    .toplevels
                    .iter()
                    .filter(|surface| !current_window_ids.contains(&surface.window_id))
                    .filter(|surface| surface.has_been_presented)
                    .map(|surface| surface.window_id.clone())
                    .collect::<Vec<_>>();
                state.drop_presented_windows(&dropped_window_ids);
                let presented_window_ids = state
                    .snapshot()
                    .known_surfaces
                    .toplevels
                    .iter()
                    .filter(|surface| surface.has_been_presented)
                    .map(|surface| surface.window_id.clone())
                    .collect::<std::collections::HashSet<_>>();
                (
                    should_render,
                    render_items,
                    window_items,
                    decoration_policies,
                    output,
                    surfaces,
                    presented_window_ids,
                    pending_new_window_ids.into_iter().collect::<HashSet<_>>(),
                    unseen_window_ids,
                    dropped_window_ids,
                    current_window_ids,
                )
            };

            for window_id in unseen_window_ids {
                self.opening_transitions
                    .entry(window_id.clone())
                    .or_insert_with(|| {
                        OpeningWindowTransition::new(
                            window_id,
                            transition_now(&self.presentation_state.clock),
                        )
                    });
            }

            let dropped_window_id_set = dropped_window_ids.iter().cloned().collect::<HashSet<_>>();
            let transition_now_value = transition_now(&self.presentation_state.clock);
            self.resize_transitions.retain(|window_id, transition| {
                (!dropped_window_id_set.contains(window_id)
                    && current_window_ids.contains(window_id))
                    || resize_transition_is_active(transition_now_value, transition)
            });
            self.pending_presented_window_ids
                .retain(|window_id| current_window_ids.contains(window_id));
            let has_close_requested_windows = window_items
                .iter()
                .any(|window| self.state().close_requested_for_window(&window.window_id));
            self.opening_transitions
                .retain(|window_id, _| current_window_ids.contains(window_id));
            let has_active_transitions = !self.resize_transitions.is_empty()
                || !self.closing_transitions.is_empty()
                || !self.opening_transitions.is_empty()
                || self.scene_transition.is_some()
                || !self.pending_presented_window_ids.is_empty()
                || has_close_requested_windows;
            if has_active_transitions {
                self.state_mut().request_redraw();
            }

            let should_render = should_render || has_active_transitions;

            if !should_render {
                self.record_frame_debug(
                    "redraw-not-requested",
                    false,
                    &window_items,
                    &surfaces,
                    &presented_window_ids,
                    &newly_presented_window_ids,
                );
                return Ok(());
            }

            let output = match output {
                Some(output) => output,
                None => {
                    self.record_frame_debug(
                        "no-output",
                        false,
                        &window_items,
                        &surfaces,
                        &presented_window_ids,
                        &newly_presented_window_ids,
                    );
                    return Ok(());
                }
            };

            let close_requested_window_ids = window_items
                .iter()
                .filter_map(|window| {
                    self.state()
                        .close_requested_for_window(&window.window_id)
                        .then(|| window.window_id.clone())
                })
                .collect::<HashSet<_>>();
            if !close_requested_window_ids.is_empty() {
                self.state_mut().request_redraw();
            }

            let backend = match self.backend.as_mut() {
                Some(backend) => backend,
                None => {
                    self.record_frame_debug(
                        "no-backend",
                        false,
                        &window_items,
                        &surfaces,
                        &presented_window_ids,
                        &newly_presented_window_ids,
                    );
                    return Ok(());
                }
            };
            let age = backend.buffer_age().unwrap_or(0);
            let frame_target = self.presentation_state.clock.now() + frame_interval(&output);

            let (result, frame_window_debug, scene_transaction_debug) =
                {
                    self.last_newly_presented_window_ids.clear();
                    self.last_snapshot_used_window_ids.clear();
                    self.last_live_window_ids.clear();
                    let mut frame_window_debug = Vec::new();
                    let render_state = self
                        .render_state
                        .get_or_insert_with(|| TitlebarRenderState::new(&output));
                    let (renderer, mut framebuffer) = backend
                        .bind()
                        .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
                    let output_scale =
                        smithay::utils::Scale::from(output.current_scale().fractional_scale());
                    let transition_now_value = transition_now(&self.presentation_state.clock);
                    for window_id in &close_requested_window_ids {
                        if self.scene_transition.as_ref().is_some_and(|transition| {
                            transition.affected_windows.contains(window_id)
                        }) {
                            continue;
                        }
                        let active_scene_visible =
                            self.scene_transition.as_ref().is_some_and(|transition| {
                                scene_transition_has_visible_alpha(
                                    transition,
                                    &window_items,
                                    &newly_presented_window_ids,
                                    transition_now_value,
                                )
                            });
                        if active_scene_visible {
                            if let Some(transition) = self.scene_transition.as_mut() {
                                widen_scene_transition_for_close(
                                    transition,
                                    &window_items,
                                    window_id,
                                    transition_now_value,
                                );
                            }
                            continue;
                        }
                        if let Some(next_transition) = build_close_scene_transition(
                            renderer,
                            &window_items,
                            &render_items,
                            &decoration_policies,
                            &surfaces,
                            window_id,
                            transition_now_value,
                            self.window_size,
                            output_scale,
                        ) {
                            self.scene_transition = Some(merge_scene_transition(
                                self.scene_transition.take(),
                                next_transition,
                            ));
                        }
                    }
                    let frozen_window_ids = self.scene_transition.as_ref().map(|transition| {
                        transition
                            .affected_windows
                            .iter()
                            .cloned()
                            .collect::<HashSet<_>>()
                    });
                    let mut overlay_emitted = false;
                    let mut elements = build_compositor_render_elements(
                        render_state,
                        renderer,
                        output_scale,
                        &render_items,
                        &window_items,
                        &decoration_policies,
                        &surfaces,
                        &presented_window_ids,
                        &newly_presented_window_ids,
                        &close_requested_window_ids,
                        &self.opening_transitions,
                        frozen_window_ids.as_ref(),
                        transition_now_value,
                        &mut self.resize_transitions,
                        &self.closing_transitions,
                        &mut self.last_snapshot_used_window_ids,
                        &mut self.last_live_window_ids,
                        &mut frame_window_debug,
                    );
                    if let Some(scene_transition) = self.scene_transition.as_mut() {
                        refresh_scene_transition_blocked_windows(
                            scene_transition,
                            &window_items,
                            &surfaces,
                            &presented_window_ids,
                            &newly_presented_window_ids,
                        );
                        let layout_changed =
                            layout_signature(&window_items) != scene_transition.captured_layouts;
                        let scene_settled = scene_transition.blocked_windows.is_empty()
                            && !scene_transition
                                .affected_windows
                                .iter()
                                .any(|window_id| newly_presented_window_ids.contains(window_id));
                        let alpha = scene_transition_alpha(
                            transition_now_value,
                            scene_transition,
                            layout_changed,
                            scene_settled,
                        );
                        let needs_overlay = scene_transition_needs_overlay(
                            scene_transition,
                            &window_items,
                            &surfaces,
                            &close_requested_window_ids,
                            layout_changed,
                        );
                        if needs_overlay && alpha > 0.0 {
                            if let Some(element) = build_scene_snapshot_element(
                                &scene_transition.snapshot,
                                alpha,
                                output_scale,
                                scene_transition.frozen_rect,
                            ) {
                                overlay_emitted = true;
                                elements.push(CompositorRenderElement::from(element));
                            }
                        }
                    }
                    let scene_transaction_debug = scene_transaction_debug_record(
                        self.scene_transition.as_ref(),
                        &window_items,
                        &surfaces,
                        &newly_presented_window_ids,
                        transition_now_value,
                        overlay_emitted,
                    );
                    let actually_presented_window_ids = newly_presented_window_ids
                        .iter()
                        .filter(|window_id| self.last_live_window_ids.contains(*window_id))
                        .cloned()
                        .collect::<HashSet<_>>();
                    self.last_newly_presented_window_ids = actually_presented_window_ids.clone();
                    for window_id in &actually_presented_window_ids {
                        self.opening_transitions.remove(window_id);
                    }
                    self.pending_presented_window_ids = newly_presented_window_ids
                        .difference(&actually_presented_window_ids)
                        .cloned()
                        .collect();
                    let result = render_state
                        .damage_tracker
                        .render_output(
                            renderer,
                            &mut framebuffer,
                            age,
                            &elements,
                            DEFAULT_CLEAR_COLOR,
                        )
                        .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
                    (result, frame_window_debug, scene_transaction_debug)
                };

            let has_rendered = result.damage.is_some();
            let submitted_damage = result.damage.cloned();
            let render_states = result.states;

            if let Some(damage) = submitted_damage.as_deref() {
                backend
                    .submit(Some(damage))
                    .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
            }
            let _ = backend;
            self.push_frame_debug(FrameDebugRecord {
                frame_index: self.next_frame_debug_index,
                rendered: true,
                reason: "rendered".into(),
                focused_window_id: self.state().snapshot().seat.focused_window_id,
                presented_window_ids: presented_window_ids.iter().cloned().collect(),
                pending_presented_window_ids: self
                    .pending_presented_window_ids
                    .iter()
                    .cloned()
                    .collect(),
                scene_transaction: scene_transaction_debug,
                windows: frame_window_debug,
            });
            self.next_frame_debug_index += 1;
            let actually_presented_window_ids = self.last_newly_presented_window_ids.clone();
            {
                let state = self.state_mut();
                for window_id in &actually_presented_window_ids {
                    state.mark_presented(window_id);
                }
                post_repaint(
                    state,
                    &output,
                    frame_target,
                    has_rendered,
                    &surfaces,
                    &render_states,
                );
            }
            let current_window_ids = self
                .state()
                .snapshot()
                .known_surfaces
                .toplevels
                .iter()
                .map(|surface| surface.window_id.clone())
                .collect::<HashSet<_>>();
            let current_window_plan = self.state().current_window_render_plan().to_vec();
            let current_surfaces = self.state().renderable_toplevel_surfaces();
            self.pending_presented_window_ids
                .retain(|window_id| current_window_ids.contains(window_id));
            if let Some(transition) = self.scene_transition.as_mut() {
                refresh_scene_transition_blocked_windows(
                    transition,
                    &current_window_plan,
                    &current_surfaces,
                    &presented_window_ids,
                    &newly_presented_window_ids,
                );
            }
            let scene_settled = self.scene_transition.as_ref().is_none_or(|transition| {
                transition.blocked_windows.is_empty()
                    && !transition
                        .affected_windows
                        .iter()
                        .any(|window_id| newly_presented_window_ids.contains(window_id))
            });
            self.opening_transitions
                .retain(|window_id, _| current_window_ids.contains(window_id));
            self.closing_transitions.retain(|window_id, transition| {
                current_window_ids.contains(window_id)
                    || closing_transition_is_active(transition_now_value, transition)
            });
            let current_layout_signature = layout_signature(&current_window_plan);
            self.scene_transition = self.scene_transition.take().and_then(|mut transition| {
                let layout_changed = current_layout_signature != transition.captured_layouts;
                if layout_changed {
                    transition.awaiting_layout_change = false;
                }
                if scene_settled {
                    transition.awaiting_settle = false;
                }
                if transition.awaiting_layout_change || transition.awaiting_settle {
                    transition.started_at = transition_now_value;
                }

                (scene_transition_alpha(
                    transition_now_value,
                    &transition,
                    layout_changed,
                    scene_settled,
                ) > 0.0)
                    .then_some(transition)
            });
            if !self.resize_transitions.is_empty()
                || !self.closing_transitions.is_empty()
                || !self.opening_transitions.is_empty()
                || self.scene_transition.is_some()
                || !self.pending_presented_window_ids.is_empty()
            {
                self.state_mut().request_redraw();
            }
            self.write_frame_debug_log();

            Ok(())
        }

        pub fn take_pending_discovery_events(
            &mut self,
        ) -> Vec<crate::backend::BackendDiscoveryEvent> {
            self.state_mut().take_discovery_events()
        }

        pub fn drain_pending_discovery_commands(&mut self) -> Vec<ControllerCommand> {
            self.take_pending_discovery_events()
                .into_iter()
                .map(ControllerCommand::DiscoveryEvent)
                .collect()
        }

        pub fn take_workspace_actions(&mut self) -> Vec<spiders_shared::api::WmAction> {
            self.state_mut().take_workspace_actions()
        }

        fn dispatch_winit_events(&mut self) -> Result<(), SmithayRuntimeError> {
            let winit = self
                .winit
                .as_mut()
                .ok_or_else(|| SmithayRuntimeError::Winit("winit event loop missing".into()))?;

            let mut pending_events = Vec::new();
            let status = winit.dispatch_new_events(|event| pending_events.push(event));
            if let smithay::reexports::winit::platform::pump_events::PumpStatus::Exit(_) = status {
                self.loop_signal.stop();
                self.stopped = true;
            }

            let mut window_size = self.window_size;
            let state = self.state_mut();
            for event in pending_events {
                handle_winit_event(state, event, &mut window_size)?;
            }
            self.window_size = window_size;
            self.apply_winit_cursor_feedback();

            Ok(())
        }
    }

    fn handle_winit_event(
        state: &mut SpidersSmithayState,
        event: WinitEvent,
        window_size: &mut (i32, i32),
    ) -> Result<(), SmithayRuntimeError> {
        match event {
            WinitEvent::Input(input) => handle_input_event(state, input, window_size),
            WinitEvent::CloseRequested => Ok(()),
            WinitEvent::Resized { size, .. } => {
                *window_size = (size.w, size.h);
                state.update_active_output_size((size.w.max(0) as u32, size.h.max(0) as u32));
                Ok(())
            }
            WinitEvent::Focus(_) | WinitEvent::Redraw => Ok(()),
        }
    }

    fn build_compositor_render_elements(
        render_state: &mut TitlebarRenderState,
        renderer: &mut GlesRenderer,
        output_scale: smithay::utils::Scale<f64>,
        titlebars: &[TitlebarRenderItem],
        windows: &[SmithayWindowRenderSnapshot],
        decoration_policies: &[(WindowId, SmithayWindowDecorationPolicySnapshot)],
        surfaces: &[SmithayRenderableToplevelSurface],
        presented_window_ids: &std::collections::HashSet<WindowId>,
        pending_presented_window_ids: &std::collections::HashSet<WindowId>,
        close_requested_window_ids: &std::collections::HashSet<WindowId>,
        opening_transitions: &HashMap<WindowId, OpeningWindowTransition>,
        frozen_window_ids: Option<&std::collections::HashSet<WindowId>>,
        transition_now_value: std::time::Duration,
        resize_transitions: &mut HashMap<WindowId, ResizeTransition>,
        closing_transitions: &HashMap<WindowId, ClosingWindowTransition>,
        used_snapshot_window_ids: &mut HashSet<WindowId>,
        used_live_window_ids: &mut HashSet<WindowId>,
        frame_window_debug: &mut Vec<FrameWindowDebugRecord>,
    ) -> Vec<CompositorRenderElement<GlesRenderer>> {
        let mut elements = Vec::new();
        let scene_transition_active = frozen_window_ids.is_some();

        for transition in closing_transitions.values() {
            if scene_transition_active {
                continue;
            }
            let window_id = &transition.window_id;
            if windows.iter().any(|window| &window.window_id == window_id) {
                continue;
            }

            elements.extend(
                build_window_border_elements(
                    &transition.snapshot.render_snapshot,
                    decoration_policies,
                )
                .into_iter()
                .map(CompositorRenderElement::from),
            );
            if let Some(element) = build_snapshot_fallback_element(
                &transition.snapshot,
                closing_transition_alpha(transition_now_value, transition),
                output_scale,
                None,
                None,
            ) {
                elements.push(CompositorRenderElement::from(element));
                used_snapshot_window_ids.insert(window_id.clone());
            }
        }

        for window in windows {
            let Some(surface) = surfaces
                .iter()
                .find(|surface| surface.window_id == window.window_id)
            else {
                continue;
            };

            let expected_width = window.window_rect.width.max(0.0).round() as i32;
            let expected_height = (window.window_rect.height - window.content_offset_y)
                .max(0.0)
                .round() as i32;
            let considered_presented = presented_window_ids.contains(&window.window_id);
            let pending_presented = pending_presented_window_ids.contains(&window.window_id);
            let committed_size = surface.committed_size;
            let committed_view_size = committed_root_surface_size(surface.surface.wl_surface());
            let scene_masking_window =
                frozen_window_ids.is_some_and(|window_ids| window_ids.contains(&window.window_id));
            if let Some(transition) = resize_transitions.get_mut(&window.window_id) {
                let effective_committed_size = committed_view_size.or(committed_size);
                if committed_matches_transition_target(effective_committed_size, transition) {
                    transition.awaiting_target_commit = false;
                }
            }
            let transition_snapshot = if scene_masking_window {
                None
            } else {
                resize_transitions
                    .get(&window.window_id)
                    .map(|transition| &transition.from)
                    .or_else(|| {
                        closing_transitions
                            .get(&window.window_id)
                            .map(|transition| &transition.snapshot)
                    })
            };
            let has_resize_snapshot = transition_snapshot.is_some();
            let snapshot_rect = transition_snapshot.map(|snapshot| {
                (
                    snapshot.render_snapshot.window_rect.x.round() as i32,
                    snapshot.render_snapshot.window_rect.y.round() as i32,
                    snapshot.render_snapshot.window_rect.width.round() as i32,
                    snapshot.render_snapshot.window_rect.height.round() as i32,
                )
            });
            let mut drew_live = false;
            let mut drew_snapshot = false;

            if !considered_presented && !pending_presented {
                frame_window_debug.push(FrameWindowDebugRecord {
                    window_id: window.window_id.clone(),
                    target_rect: (
                        window.window_rect.x.round() as i32,
                        window.window_rect.y.round() as i32,
                        window.window_rect.width.round() as i32,
                        window.window_rect.height.round() as i32,
                    ),
                    committed_size,
                    committed_view_size,
                    has_buffer: surface.has_buffer,
                    has_resize_snapshot,
                    snapshot_rect,
                    considered_presented,
                    pending_presented,
                    drew_live,
                    drew_snapshot,
                });
                continue;
            }

            let location = (
                window.window_rect.x.round() as i32,
                (window.window_rect.y + window.content_offset_y).round() as i32,
            );
            let render_alpha = opening_transitions
                .get(&window.window_id)
                .map(|transition| opening_transition_alpha(transition_now_value, transition))
                .unwrap_or(1.0);
            let resize_transition_progress_value = resize_transitions
                .get(&window.window_id)
                .map(|transition| resize_transition_progress(transition_now_value, transition))
                .unwrap_or(1.0);
            let surface_elements = render_elements_from_surface_tree::<
                GlesRenderer,
                WaylandSurfaceRenderElement<GlesRenderer>,
            >(
                renderer,
                surface.surface.wl_surface(),
                location,
                output_scale,
                render_alpha * resize_transition_progress_value.max(0.01),
                Kind::Unspecified,
            );
            let constrain_rect =
                Rectangle::new(location.into(), (expected_width, expected_height).into());
            if constrain_rect.size.w <= 0 || constrain_rect.size.h <= 0 {
                continue;
            }

            let close_requested = close_requested_window_ids.contains(&window.window_id);
            let has_live_content = surface.has_buffer && !surface_elements.is_empty();
            if !has_live_content {
                if let Some(snapshot) = transition_snapshot {
                    elements.extend(
                        build_window_border_elements_for_rect(
                            window.window_id.clone(),
                            window.window_rect,
                            decoration_policies,
                        )
                        .into_iter()
                        .map(CompositorRenderElement::from),
                    );
                    if let Some(element) = build_snapshot_fallback_element(
                        snapshot,
                        1.0,
                        output_scale,
                        Some(location),
                        Some((expected_width, expected_height)),
                    ) {
                        elements.push(CompositorRenderElement::from(element));
                        used_snapshot_window_ids.insert(window.window_id.clone());
                        drew_snapshot = true;
                    }
                }
                frame_window_debug.push(FrameWindowDebugRecord {
                    window_id: window.window_id.clone(),
                    target_rect: (
                        window.window_rect.x.round() as i32,
                        window.window_rect.y.round() as i32,
                        window.window_rect.width.round() as i32,
                        window.window_rect.height.round() as i32,
                    ),
                    committed_size,
                    committed_view_size,
                    has_buffer: surface.has_buffer,
                    has_resize_snapshot,
                    snapshot_rect,
                    considered_presented,
                    pending_presented,
                    drew_live,
                    drew_snapshot,
                });
                continue;
            }

            if let Some((committed_width, committed_height)) =
                committed_root_surface_size(surface.surface.wl_surface()).or(surface.committed_size)
            {
                let needs_resize_preview = committed_width.max(1) != expected_width.max(1)
                    || committed_height.max(1) != expected_height.max(1);
                let awaiting_target_commit = resize_transitions
                    .get(&window.window_id)
                    .is_some_and(|transition| transition.awaiting_target_commit);
                let should_prefer_transition = !scene_masking_window
                    && (needs_resize_preview
                        || awaiting_target_commit
                        || (!opening_transitions.is_empty()
                            && resize_transitions.contains_key(&window.window_id)));
                if should_prefer_transition && committed_width > 0 && committed_height > 0 {
                    if let Some(snapshot) = resize_transitions
                        .get(&window.window_id)
                        .map(|transition| &transition.from)
                        .or_else(|| {
                            closing_transitions
                                .get(&window.window_id)
                                .map(|transition| &transition.snapshot)
                        })
                    {
                        let snapshot_alpha =
                            if let Some(transition) = resize_transitions.get(&window.window_id) {
                                1.0 - resize_transition_progress(transition_now_value, transition)
                            } else {
                                1.0
                            };
                        elements.extend(
                            build_window_border_elements_for_rect(
                                window.window_id.clone(),
                                window.window_rect,
                                decoration_policies,
                            )
                            .into_iter()
                            .map(CompositorRenderElement::from),
                        );
                        if let Some(element) = build_snapshot_fallback_element(
                            snapshot,
                            snapshot_alpha,
                            output_scale,
                            Some(location),
                            Some((expected_width, expected_height)),
                        ) {
                            elements.push(CompositorRenderElement::from(element));
                            used_snapshot_window_ids.insert(window.window_id.clone());
                            drew_snapshot = true;
                        }
                    }
                    frame_window_debug.push(FrameWindowDebugRecord {
                        window_id: window.window_id.clone(),
                        target_rect: (
                            window.window_rect.x.round() as i32,
                            window.window_rect.y.round() as i32,
                            window.window_rect.width.round() as i32,
                            window.window_rect.height.round() as i32,
                        ),
                        committed_size,
                        committed_view_size,
                        has_buffer: surface.has_buffer,
                        has_resize_snapshot,
                        snapshot_rect,
                        considered_presented,
                        pending_presented,
                        drew_live,
                        drew_snapshot,
                    });
                    continue;
                }
            }

            if !close_requested {
                let effective_committed_size = committed_view_size.or(committed_size);
                let should_release_transition = resize_transitions
                    .get(&window.window_id)
                    .is_some_and(|transition| {
                        resize_transition_can_release(
                            effective_committed_size,
                            transition,
                            transition_now_value,
                        )
                    });
                if should_release_transition {
                    resize_transitions.remove(&window.window_id);
                }
            }
            elements.extend(
                build_window_border_elements(window, decoration_policies)
                    .into_iter()
                    .map(CompositorRenderElement::from),
            );

            elements.extend(
                surface_elements
                    .into_iter()
                    .filter_map(|element| {
                        CropRenderElement::from_element(element, output_scale, constrain_rect)
                    })
                    .map(CompositorRenderElement::from),
            );
            used_live_window_ids.insert(window.window_id.clone());
            drew_live = true;
            frame_window_debug.push(FrameWindowDebugRecord {
                window_id: window.window_id.clone(),
                target_rect: (
                    window.window_rect.x.round() as i32,
                    window.window_rect.y.round() as i32,
                    window.window_rect.width.round() as i32,
                    window.window_rect.height.round() as i32,
                ),
                committed_size,
                committed_view_size,
                has_buffer: surface.has_buffer,
                has_resize_snapshot,
                snapshot_rect,
                considered_presented,
                pending_presented,
                drew_live,
                drew_snapshot,
            });
        }

        elements.extend(
            titlebars
                .iter()
                .filter(|item| {
                    presented_window_ids.contains(&item.window_id)
                        || pending_presented_window_ids.contains(&item.window_id)
                })
                .filter_map(|item| {
                    let color = titlebar_background_color(item);
                    let width = item.titlebar_rect.width.max(0.0).round() as i32;
                    let height = item.titlebar_rect.height.max(0.0).round() as i32;
                    if width <= 0 || height <= 0 {
                        return None;
                    }

                    let rect = Rectangle::new(
                        (
                            item.titlebar_rect.x.round() as i32,
                            item.titlebar_rect.y.round() as i32,
                        )
                            .into(),
                        (width, height).into(),
                    );
                    Some(SolidColorRenderElement::new(
                        render_state.next_element_id(),
                        rect,
                        1usize,
                        color,
                        Kind::Unspecified,
                    ))
                })
                .map(CompositorRenderElement::from),
        );

        elements.extend(
            titlebars
                .iter()
                .filter_map(build_titlebar_border_element)
                .map(CompositorRenderElement::from),
        );

        elements.extend(
            titlebars
                .iter()
                .filter_map(|item| build_titlebar_text_element(renderer, item))
                .map(CompositorRenderElement::from),
        );

        elements
    }

    fn build_titlebar_text_element(
        renderer: &mut GlesRenderer,
        item: &TitlebarRenderItem,
    ) -> Option<MemoryRenderBufferRenderElement<GlesRenderer>> {
        let font_scale = titlebar_font_scale(item);
        let max_text_width = titlebar_available_text_width(item);
        let text = truncate_titlebar_text(&titlebar_text(item), font_scale, max_text_width);
        if text.is_empty() {
            return None;
        }

        let glyph_width = 8 * font_scale;
        let glyph_height = 8 * font_scale;
        let text_width = (text.chars().count() as i32) * glyph_width;
        let text_height = glyph_height;
        let available_width = max_text_width;
        let available_height = item.titlebar_rect.height.max(0.0).round() as i32;
        if available_width <= 0 || available_height <= 0 || text_width <= 0 || text_height <= 0 {
            return None;
        }

        let width = text_width.min(available_width);
        let height = text_height.min(available_height);
        if width <= 0 || height <= 0 {
            return None;
        }

        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let color = titlebar_text_color(item);
        let rgba = [
            (color[0] * 255.0).round().clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).round().clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).round().clamp(0.0, 255.0) as u8,
            (color[3] * 255.0).round().clamp(0.0, 255.0) as u8,
        ];
        draw_bitmap_text(&mut pixels, width, height, &text, font_scale, rgba);

        let buffer = MemoryRenderBuffer::from_slice(
            &pixels,
            Fourcc::Abgr8888,
            (width, height),
            1,
            Transform::Normal,
            None,
        );
        let location = titlebar_text_location(item, width, height);
        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            &buffer,
            None,
            None,
            None,
            Kind::Unspecified,
        )
        .ok()
    }

    fn build_titlebar_border_element(item: &TitlebarRenderItem) -> Option<SolidColorRenderElement> {
        let width = item
            .style
            .border_bottom_width
            .as_deref()
            .and_then(parse_px_i32)?;
        if item
            .style
            .border_bottom_style
            .as_deref()
            .map(str::trim)
            .is_some_and(|style| style.eq_ignore_ascii_case("none"))
        {
            return None;
        }

        let color = item
            .style
            .border_bottom_color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| Color32F::new(0.25, 0.27, 0.30, 1.0));
        let rect = Rectangle::new(
            (
                item.titlebar_rect.x.round() as i32,
                (item.titlebar_rect.y + item.titlebar_rect.height - width as f32).round() as i32,
            )
                .into(),
            ((item.titlebar_rect.width.max(0.0).round() as i32), width).into(),
        );
        Some(SolidColorRenderElement::new(
            Id::new(),
            rect,
            1usize,
            color,
            Kind::Unspecified,
        ))
    }

    fn build_window_border_elements(
        item: &SmithayWindowRenderSnapshot,
        decoration_policies: &[(WindowId, SmithayWindowDecorationPolicySnapshot)],
    ) -> Vec<SolidColorRenderElement> {
        build_window_border_elements_for_rect(
            item.window_id.clone(),
            item.window_rect,
            decoration_policies,
        )
    }

    fn build_window_border_elements_for_rect(
        window_id: WindowId,
        window_rect: LayoutRect,
        decoration_policies: &[(WindowId, SmithayWindowDecorationPolicySnapshot)],
    ) -> Vec<SolidColorRenderElement> {
        let Some(style) = decoration_policies
            .iter()
            .find(|(candidate_window_id, _)| *candidate_window_id == window_id)
            .map(|(_, policy)| &policy.window_style)
        else {
            return Vec::new();
        };

        let Some(width) = style.border_width.as_deref().and_then(parse_px_i32) else {
            return Vec::new();
        };
        let Some(mut color) = style.border_color.as_deref().and_then(parse_hex_color) else {
            return Vec::new();
        };
        if let Some(opacity) = style.opacity.as_deref().and_then(parse_opacity_f32) {
            color = Color32F::new(
                color.r(),
                color.g(),
                color.b(),
                (color.a() * opacity).clamp(0.0, 1.0),
            );
        }

        let x = window_rect.x.round() as i32;
        let y = window_rect.y.round() as i32;
        let w = window_rect.width.max(0.0).round() as i32;
        let h = window_rect.height.max(0.0).round() as i32;
        if w <= 0 || h <= 0 {
            return Vec::new();
        }

        let vertical_height = (h - 2 * width).max(0);
        let rects = [
            Rectangle::new((x, y).into(), (w, width).into()),
            Rectangle::new((x, y + h - width).into(), (w, width).into()),
            Rectangle::new((x, y + width).into(), (width, vertical_height).into()),
            Rectangle::new(
                (x + w - width, y + width).into(),
                (width, vertical_height).into(),
            ),
        ];

        rects
            .into_iter()
            .filter(|rect| rect.size.w > 0 && rect.size.h > 0)
            .map(|rect| {
                SolidColorRenderElement::new(Id::new(), rect, 1usize, color, Kind::Unspecified)
            })
            .collect()
    }

    fn titlebar_font_scale(item: &TitlebarRenderItem) -> i32 {
        let size = item
            .style
            .font_size
            .as_deref()
            .and_then(parse_px_i32)
            .unwrap_or(12);
        (size / 8).max(1)
    }

    fn titlebar_text(item: &TitlebarRenderItem) -> String {
        match item.style.text_transform.as_deref().map(str::trim) {
            Some("uppercase") => item.title.to_uppercase(),
            Some("lowercase") => item.title.to_lowercase(),
            _ => item.title.clone(),
        }
    }

    fn titlebar_available_text_width(item: &TitlebarRenderItem) -> i32 {
        let padding = item
            .style
            .padding
            .as_deref()
            .and_then(parse_px_i32)
            .unwrap_or(8);
        (item.titlebar_rect.width.max(0.0).round() as i32).saturating_sub(padding * 2)
    }

    fn truncate_titlebar_text(text: &str, font_scale: i32, max_width: i32) -> String {
        let max_chars = max_width / (8 * font_scale.max(1));
        if max_chars <= 0 {
            return String::new();
        }

        let chars = text.chars().collect::<Vec<_>>();
        if chars.len() as i32 <= max_chars {
            return text.to_owned();
        }
        if max_chars <= 3 {
            return ".".repeat(max_chars as usize);
        }

        let visible = (max_chars - 3) as usize;
        let mut truncated = chars.into_iter().take(visible).collect::<String>();
        truncated.push_str("...");
        truncated
    }

    fn titlebar_text_color(item: &TitlebarRenderItem) -> [f32; 4] {
        item.style
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .map(|color| [color.r(), color.g(), color.b(), color.a()])
            .unwrap_or([0.93, 0.94, 0.96, 1.0])
    }

    fn titlebar_text_location(
        item: &TitlebarRenderItem,
        width: i32,
        height: i32,
    ) -> Point<f64, smithay::utils::Physical> {
        let padding = item
            .style
            .padding
            .as_deref()
            .and_then(parse_px_i32)
            .unwrap_or(8);
        let x = match item.style.text_align.as_deref().map(str::trim) {
            Some("center") => {
                item.titlebar_rect.x.round() as i32
                    + (((item.titlebar_rect.width.round() as i32) - width) / 2).max(0)
            }
            Some("right") => {
                item.titlebar_rect.x.round() as i32
                    + ((item.titlebar_rect.width.round() as i32) - width - padding).max(0)
            }
            _ => item.titlebar_rect.x.round() as i32 + padding,
        };
        let y = item.titlebar_rect.y.round() as i32
            + (((item.titlebar_rect.height.round() as i32) - height) / 2).max(0);
        Point::from((f64::from(x), f64::from(y)))
    }

    fn draw_bitmap_text(
        pixels: &mut [u8],
        width: i32,
        height: i32,
        text: &str,
        scale: i32,
        color: [u8; 4],
    ) {
        let mut pen_x = 0;
        for ch in text.chars() {
            let Some(glyph) = BASIC_FONTS.get(ch) else {
                pen_x += 8 * scale;
                continue;
            };
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if (bits >> col) & 1 == 0 {
                        continue;
                    }
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let x = pen_x + ((7 - col) * scale) + sx;
                            let y = (row as i32 * scale) + sy;
                            if x < 0 || x >= width || y < 0 || y >= height {
                                continue;
                            }
                            let idx = ((y * width + x) * 4) as usize;
                            pixels[idx..idx + 4].copy_from_slice(&color);
                        }
                    }
                }
            }
            pen_x += 8 * scale;
            if pen_x >= width {
                break;
            }
        }
    }

    fn parse_px_i32(value: &str) -> Option<i32> {
        let value = value.trim().strip_suffix("px").unwrap_or(value).trim();
        value.parse::<i32>().ok().filter(|value| *value > 0)
    }

    fn parse_opacity_f32(value: &str) -> Option<f32> {
        value
            .trim()
            .parse::<f32>()
            .ok()
            .map(|value| value.clamp(0.0, 1.0))
    }

    fn post_repaint(
        state: &mut SpidersSmithayState,
        output: &Output,
        frame_target: smithay::utils::Time<Monotonic>,
        has_rendered: bool,
        surfaces: &[SmithayRenderableToplevelSurface],
        render_states: &smithay::backend::renderer::element::RenderElementStates,
    ) {
        for surface in surfaces {
            send_frames_surface_tree(
                surface.surface.wl_surface(),
                output,
                frame_target,
                Some(Duration::ZERO),
                |_, _| Some(output.clone()),
            );
        }

        if has_rendered {
            let mut feedback = OutputPresentationFeedback::new(output);
            for surface in surfaces {
                take_presentation_feedback_surface_tree(
                    surface.surface.wl_surface(),
                    &mut feedback,
                    |_, _| Some(output.clone()),
                    |surface, _| {
                        surface_presentation_feedback_flags_from_states(surface, render_states)
                    },
                );
            }
            feedback.presented(
                frame_target,
                refresh_interval(output),
                0,
                wp_presentation_feedback::Kind::Vsync,
            );
        }

        let _ = state.display_handle.flush_clients();
    }

    fn frame_interval(output: &Output) -> Duration {
        output
            .current_mode()
            .map(|mode| Duration::from_secs_f64(1_000f64 / f64::from(mode.refresh)))
            .unwrap_or_default()
    }

    fn refresh_interval(output: &Output) -> Refresh {
        output
            .current_mode()
            .map(|mode| Refresh::fixed(Duration::from_secs_f64(1_000f64 / f64::from(mode.refresh))))
            .unwrap_or(Refresh::Unknown)
    }

    fn titlebar_background_color(item: &TitlebarRenderItem) -> Color32F {
        item.style
            .background
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or_else(|| {
                if item.focused {
                    Color32F::new(0.18, 0.20, 0.24, 1.0)
                } else {
                    Color32F::new(0.13, 0.14, 0.16, 1.0)
                }
            })
    }

    fn cursor_icon_for_snapshot(cursor_image: &str) -> CursorIcon {
        match cursor_image.strip_prefix("named:").unwrap_or(cursor_image) {
            "Grab" => CursorIcon::Grab,
            "Grabbing" => CursorIcon::Grabbing,
            "NsResize" => CursorIcon::NsResize,
            "EwResize" => CursorIcon::EwResize,
            "NwseResize" => CursorIcon::NwseResize,
            "NeswResize" => CursorIcon::NeswResize,
            "Crosshair" => CursorIcon::Crosshair,
            _ => CursorIcon::Default,
        }
    }

    fn parse_hex_color(value: &str) -> Option<Color32F> {
        let hex = value.trim().strip_prefix('#')?;
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color32F::new(
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    1.0,
                ))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Color32F::new(
                    f32::from(r) / 255.0,
                    f32::from(g) / 255.0,
                    f32::from(b) / 255.0,
                    f32::from(a) / 255.0,
                ))
            }
            _ => None,
        }
    }

    fn handle_input_event<I>(
        state: &mut SpidersSmithayState,
        event: InputEvent<I>,
        window_size: &mut (i32, i32),
    ) -> Result<(), SmithayRuntimeError>
    where
        I: smithay::backend::input::InputBackend,
    {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let keyboard = state.seat.get_keyboard().ok_or_else(|| {
                    SmithayRuntimeError::Winit("smithay keyboard capability missing".into())
                })?;
                let serial = SERIAL_COUNTER.next_serial();

                keyboard.input::<(), _>(
                    state,
                    event.key_code(),
                    event.state(),
                    serial,
                    event.time_msec(),
                    |state, modifiers, handle| {
                        if event.state() == smithay::backend::input::KeyState::Pressed {
                            let keysym = handle.modified_sym();
                            if let Some(action) = state
                                .bindings()
                                .iter()
                                .find(|binding| {
                                    binding_matches(&binding.trigger, *modifiers, keysym)
                                })
                                .map(|binding| binding.action.clone())
                            {
                                state.queue_workspace_action(action);
                                return FilterResult::Intercept(());
                            }
                        }

                        FilterResult::Forward
                    },
                );

                Ok(())
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let location = event.position_transformed((*window_size).into());
                state.update_pointer_location(location.x, location.y);
                state.update_titlebar_cursor_feedback();

                if state.has_active_titlebar_interaction() {
                    state.update_titlebar_interaction();
                    return Ok(());
                }

                if state.sloppyfocus() {
                    if let Some(window_id) = state.window_at_point(location.x, location.y) {
                        let focused = state.snapshot().seat.focused_window_id.clone();
                        if focused.as_ref() != Some(&window_id) {
                            state.set_keyboard_focus_for_window(&window_id);
                            state.queue_workspace_action(WmAction::FocusWindow { window_id });
                        }
                    }
                }

                let pointer = state.seat.get_pointer().ok_or_else(|| {
                    SmithayRuntimeError::Winit("smithay pointer capability missing".into())
                })?;
                let serial = SERIAL_COUNTER.next_serial();

                pointer.motion(
                    state,
                    None::<(WlSurface, Point<f64, smithay::utils::Logical>)>,
                    &MotionEvent {
                        location,
                        serial,
                        time: event.time_msec(),
                    },
                );
                pointer.frame(state);

                Ok(())
            }
            InputEvent::PointerButton { event, .. } => {
                if event.button_code() == BTN_LEFT && event.state() == ButtonState::Pressed {
                    if let Some(hit) = state.titlebar_hit_target_at_pointer() {
                        state.focus_window_from_titlebar(&hit.window_id);
                        state.note_titlebar_pointer_request(&hit.window_id, hit.kind);
                        let _ = state.begin_titlebar_interaction(&hit);
                        return Ok(());
                    }
                }

                if event.button_code() == BTN_LEFT && event.state() == ButtonState::Released {
                    if let Some((window_id, rect)) = state.end_titlebar_interaction() {
                        state.update_titlebar_cursor_feedback();
                        state.queue_workspace_action(WmAction::SetFloatingWindowGeometry {
                            window_id,
                            rect,
                        });
                        return Ok(());
                    }
                }

                let pointer = state.seat.get_pointer().ok_or_else(|| {
                    SmithayRuntimeError::Winit("smithay pointer capability missing".into())
                })?;
                let serial = SERIAL_COUNTER.next_serial();

                pointer.button(
                    state,
                    &ButtonEvent {
                        serial,
                        time: event.time_msec(),
                        button: event.button_code(),
                        state: event.state(),
                    },
                );
                pointer.frame(state);

                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn snapshot_topology(topology: &CompositorTopologyState) -> SmithayBootstrapTopologySnapshot {
        SmithayBootstrapTopologySnapshot {
            active_output_id: topology.active_output_id.clone(),
            active_seat_name: topology.active_seat_name.clone(),
            outputs: topology.outputs.clone(),
            seats: topology.seats.clone(),
            surfaces: topology.surfaces.clone(),
        }
    }

    fn spawn_shell_command(
        command: &str,
        wayland_display: &str,
    ) -> Result<(), SmithayRuntimeError> {
        std::process::Command::new("sh")
            .arg("-lc")
            .arg(command)
            .env("WAYLAND_DISPLAY", wayland_display)
            .spawn()
            .map(|_| ())
            .map_err(|error| SmithayRuntimeError::Winit(format!("spawn failed: {error}")))
    }

    fn binding_matches(trigger: &str, modifiers: ModifiersState, keysym: Keysym) -> bool {
        let mut needs_alt = false;
        let mut needs_ctrl = false;
        let mut needs_shift = false;
        let mut needs_logo = false;
        let mut key = None;

        for part in trigger.split('+') {
            match part {
                "alt" => needs_alt = true,
                "ctrl" | "control" => needs_ctrl = true,
                "shift" => needs_shift = true,
                "logo" | "super" | "meta" => needs_logo = true,
                other => key = Some(other),
            }
        }

        if modifiers.alt != needs_alt
            || modifiers.ctrl != needs_ctrl
            || modifiers.shift != needs_shift
            || modifiers.logo != needs_logo
        {
            return false;
        }

        let Some(expected_key) = key else {
            return false;
        };

        let actual = xkb::keysym_get_name(keysym);
        if actual == expected_key {
            return true;
        }

        actual.eq_ignore_ascii_case(expected_key)
    }

    fn logical_output_size(size: (i32, i32), scale_factor: f64) -> (u32, u32) {
        let scale = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };

        let width = ((size.0.max(0) as f64) / scale).round().max(0.0) as u32;
        let height = ((size.1.max(0) as f64) / scale).round().max(0.0) as u32;
        (width, height)
    }

    fn smithay_output_snapshot(
        output_name: &str,
        size: (i32, i32),
        scale_factor: f64,
    ) -> OutputSnapshot {
        let logical_size = logical_output_size(size, scale_factor);
        OutputSnapshot {
            id: OutputId::from(output_name),
            name: output_name.into(),
            logical_x: 0,
            logical_y: 0,
            logical_width: logical_size.0,
            logical_height: logical_size.1,
            scale: 1,
            transform: spiders_shared::wm::OutputTransform::Normal,
            enabled: true,
            current_workspace_id: None,
        }
    }

    fn initial_winit_discovery_command(
        seat_name: &str,
        output_name: &str,
        size: (i32, i32),
        scale_factor: f64,
    ) -> ControllerCommand {
        SmithayAdapter::translate_snapshot(
            1,
            vec![SmithayAdapter::translate_seat_descriptor(
                initial_winit_seat_descriptor(seat_name),
            )],
            vec![crate::backend::BackendOutputSnapshot {
                snapshot: smithay_output_snapshot(output_name, size, scale_factor),
                active: true,
            }],
            Vec::new(),
        )
    }

    fn initial_winit_seat_descriptor(seat_name: &str) -> SmithaySeatDescriptor {
        SmithaySeatDescriptor {
            seat_name: seat_name.into(),
            active: true,
        }
    }

    pub fn refresh_workspace_export_from_controller<R>(
        controller: &crate::CompositorController<R>,
        state: &mut SpidersSmithayState,
    ) where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        let snapshot = controller.state_snapshot();
        let window_placements = controller.app().session().current_window_placements();
        let titlebar_plan = controller.app().session().current_titlebar_render_plan();
        state.refresh_workspace_state(&snapshot);
        state.refresh_workspace_output_groups();
        state.refresh_titlebar_render_plan(&titlebar_plan);
        state.refresh_window_render_plan(&build_window_render_plan(
            &window_placements,
            &titlebar_plan,
        ));

        let decoration_policies = controller.app().window_decoration_policies();
        state.refresh_window_decoration_policies(
            &decoration_policies
                .into_iter()
                .map(|(window_id, policy)| {
                    (
                        window_id,
                        SmithayWindowDecorationPolicySnapshot {
                            decorations_visible: policy.decorations_visible,
                            titlebar_visible: policy.titlebar_visible,
                            window_style: policy.window_style,
                            titlebar_style: policy.titlebar_style,
                        },
                    )
                })
                .collect::<Vec<_>>(),
        );
    }

    fn build_window_render_plan(
        window_placements: &[crate::runtime::WindowPlacement],
        titlebar_plan: &[TitlebarRenderItem],
    ) -> Vec<SmithayWindowRenderSnapshot> {
        window_placements
            .iter()
            .map(|placement| SmithayWindowRenderSnapshot {
                window_id: placement.window_id.clone(),
                window_rect: placement.rect,
                content_offset_y: titlebar_plan
                    .iter()
                    .find(|item| item.window_id == placement.window_id)
                    .map(|item| item.titlebar_rect.height)
                    .unwrap_or(0.0),
            })
            .collect()
    }

    pub fn initialize_smithay_workspace_export<R>(
        controller: &crate::CompositorController<R>,
        state: &mut SpidersSmithayState,
    ) where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        refresh_workspace_export_from_controller(controller, state);
    }

    pub fn initialize_winit_controller<R>(
        authoring_layout_service: spiders_config::authoring_layout::AuthoringLayoutService<R>,
        config: spiders_config::model::Config,
        state: spiders_shared::wm::StateSnapshot,
    ) -> Result<crate::CompositorController<R>, SmithayRuntimeError>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        crate::CompositorController::initialize(authoring_layout_service, config, state)
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))
    }

    pub fn bootstrap_winit<R>(
        authoring_layout_service: spiders_config::authoring_layout::AuthoringLayoutService<R>,
        config: spiders_config::model::Config,
        state: spiders_shared::wm::StateSnapshot,
    ) -> Result<SmithayBootstrap<R>, SmithayRuntimeError>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        bootstrap_winit_with_options(
            authoring_layout_service,
            config,
            state,
            SmithayWinitOptions::default(),
        )
    }

    pub fn bootstrap_winit_with_options<R>(
        authoring_layout_service: spiders_config::authoring_layout::AuthoringLayoutService<R>,
        config: spiders_config::model::Config,
        state: spiders_shared::wm::StateSnapshot,
        options: SmithayWinitOptions,
    ) -> Result<SmithayBootstrap<R>, SmithayRuntimeError>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        let mut controller = initialize_winit_controller(authoring_layout_service, config, state)?;
        let (runtime, report) = bootstrap_winit_controller_with_options(&mut controller, options)?;

        Ok(SmithayBootstrap {
            controller,
            runtime,
            report,
            lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
        })
    }

    pub fn bootstrap_winit_controller<R>(
        controller: &mut crate::CompositorController<R>,
    ) -> Result<(SmithayWinitRuntime<'static>, SmithayStartupReport), SmithayRuntimeError>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        bootstrap_winit_controller_with_options(controller, SmithayWinitOptions::default())
    }

    pub fn bootstrap_winit_controller_with_options<R>(
        controller: &mut crate::CompositorController<R>,
        options: SmithayWinitOptions,
    ) -> Result<(SmithayWinitRuntime<'static>, SmithayStartupReport), SmithayRuntimeError>
    where
        R: AuthoringLayoutRuntime<Config = Config>,
    {
        let event_loop = EventLoop::<SpidersSmithayState>::try_new()
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let display =
            Display::new().map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let mut smithay_state = SpidersSmithayState::new(&display, "smithay-winit")?;
        smithay_state.set_bindings(controller.app().session().config().bindings.clone());
        smithay_state.set_sloppyfocus(
            controller
                .app()
                .session()
                .config()
                .options
                .sloppyfocus
                .unwrap_or(false),
        );
        let socket = smithay_state.bind_socket_source(options.socket_name.as_deref())?;
        let socket_name = socket.socket_name().to_string_lossy().into_owned();

        event_loop
            .handle()
            .insert_source(socket, |client_stream, _, state| {
                let _ = state
                    .display_handle
                    .insert_client(client_stream, Arc::new(SmithayClientState::default()));
            })
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

        event_loop
            .handle()
            .insert_source(
                Generic::new(display, Interest::READ, CalloopMode::Level),
                |_, display, state| {
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }

                    Ok(PostAction::Continue)
                },
            )
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;

        let (backend, winit) = winit::init::<GlesRenderer>()
            .map_err(|error| SmithayRuntimeError::Winit(error.to_string()))?;
        let size = backend.window_size();
        let scale_factor = backend.scale_factor();
        let logical_size = logical_output_size((size.w, size.h), scale_factor);

        let seat_name = String::from("smithay-winit");
        let output_name = String::from("smithay-winit-output");
        let smithay_output = Output::new(
            output_name.clone(),
            PhysicalProperties {
                size: (size.w, size.h).into(),
                subpixel: Subpixel::Unknown,
                make: "Spiders".into(),
                model: "Winit".into(),
                serial_number: "Bootstrap".into(),
            },
        );
        let mode = Mode {
            size: (size.w, size.h).into(),
            refresh: 60_000,
        };
        let output_scale = smithay::output::Scale::Fractional(scale_factor);

        smithay_output.change_current_state(
            Some(mode),
            Some(Transform::Flipped180),
            Some(output_scale),
            Some((0, 0).into()),
        );
        smithay_output.set_preferred(mode);
        let _global =
            smithay_output.create_global::<SpidersSmithayState>(&smithay_state.display_handle);

        smithay_state.register_smithay_output(
            OutputId::from(output_name.as_str()),
            smithay_output,
            None,
            Some(logical_size),
            true,
        );

        let command = initial_winit_discovery_command(
            &seat_name,
            &output_name,
            (size.w, size.h),
            scale_factor,
        );

        match command {
            ControllerCommand::DiscoverySnapshot(snapshot) => {
                let _ = (size.w, size.h, logical_size);
                controller.apply_command(ControllerCommand::DiscoverySnapshot(snapshot))?;
            }
            other => {
                controller.apply_command(other)?;
            }
        }

        initialize_smithay_workspace_export(controller, &mut smithay_state);

        let runtime = SmithayWinitRuntime {
            display_handle: smithay_state.display_handle.clone(),
            loop_signal: event_loop.get_signal(),
            event_loop,
            socket_name: socket_name.clone(),
            window_size: (size.w, size.h),
            state: Some(smithay_state),
            render_state: None,
            presentation_state: PresentationRenderState::new(),
            scene_transition: None,
            resize_transitions: HashMap::new(),
            closing_transitions: HashMap::new(),
            opening_transitions: HashMap::new(),
            pending_presented_window_ids: HashSet::new(),
            frame_debug_history: VecDeque::new(),
            next_frame_debug_index: 0,
            last_snapshot_capture_window_ids: HashSet::new(),
            last_snapshot_used_window_ids: HashSet::new(),
            last_live_window_ids: HashSet::new(),
            last_newly_presented_window_ids: HashSet::new(),
            backend: Some(backend),
            winit: Some(winit),
            stopped: false,
        };

        Ok((
            runtime,
            SmithayStartupReport {
                controller: controller.report(),
                output_name,
                seat_name,
                logical_size: (logical_size.0 as i32, logical_size.1 as i32),
                socket_name: Some(socket_name),
            },
        ))
    }

    #[cfg(test)]
    mod tests {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        use spiders_config::authoring_layout::AuthoringLayoutService;
        use spiders_config::model::{Config, LayoutDefinition};
        use spiders_runtime_js::loader::{RuntimePathResolver, RuntimeProjectLayoutSourceLoader};
        use spiders_runtime_js::runtime::QuickJsPreparedLayoutRuntime;
        use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
        use spiders_shared::wm::{
            LayoutRef, OutputSnapshot, OutputTransform, StateSnapshot, WorkspaceSnapshot,
        };
        use spiders_wm::{
            ControllerPhase, LayerExclusiveZone, LayerKeyboardInteractivity, LayerSurfaceMetadata,
            LayerSurfaceTier, SurfaceRole,
        };

        use super::*;

        type TestLoader = RuntimeProjectLayoutSourceLoader;
        type TestLayoutRuntime = QuickJsPreparedLayoutRuntime<TestLoader>;
        type TestBootstrap = SmithayBootstrap<TestLayoutRuntime>;

        fn test_state_snapshot() -> StateSnapshot {
            StateSnapshot {
                focused_window_id: None,
                current_output_id: Some(OutputId::from("out-1")),
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
                outputs: vec![OutputSnapshot {
                    id: OutputId::from("out-1"),
                    name: "HDMI-A-1".into(),
                    logical_x: 0,
                    logical_y: 0,
                    logical_width: 800,
                    logical_height: 600,
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
                    effective_layout: Some(LayoutRef {
                        name: "master-stack".into(),
                    }),
                }],
                windows: vec![],
                visible_window_ids: vec![],
                tag_names: vec!["1".into()],
            }
        }

        fn test_config() -> Config {
            Config {
                layouts: vec![LayoutDefinition {
                    name: "master-stack".into(),
                    module: "layouts/master-stack.js".into(),
                    stylesheet: String::new(),
                    effects_stylesheet: String::new(),
                    runtime_graph: None,
                }],
                ..Config::default()
            }
        }

        fn test_authoring_layout_service() -> AuthoringLayoutService<TestLayoutRuntime> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let runtime_root = std::env::temp_dir().join(format!(
                "spiders-smithay-runtime-test-{}-{}",
                std::process::id(),
                unique
            ));

            fs::create_dir_all(runtime_root.join("layouts")).unwrap();
            fs::write(
                runtime_root.join("layouts/master-stack.js"),
                "ctx => ({ type: 'workspace', children: [] })",
            )
            .unwrap();

            let loader =
                RuntimeProjectLayoutSourceLoader::new(RuntimePathResolver::new(".", &runtime_root));
            let runtime = QuickJsPreparedLayoutRuntime::with_loader(loader.clone());
            AuthoringLayoutService::new(runtime)
        }

        fn test_runtime(socket_name: &str) -> SmithayWinitRuntime<'static> {
            let event_loop = EventLoop::<SpidersSmithayState>::try_new().unwrap();
            let display = Display::new().unwrap();
            let state = SpidersSmithayState::new(&display, "smithay-test-seat").unwrap();

            SmithayWinitRuntime {
                display_handle: state.display_handle.clone(),
                loop_signal: event_loop.get_signal(),
                event_loop,
                socket_name: socket_name.into(),
                window_size: (1280, 720),
                state: Some(state),
                render_state: None,
                presentation_state: PresentationRenderState::new(),
                scene_transition: None,
                resize_transitions: HashMap::new(),
                closing_transitions: HashMap::new(),
                opening_transitions: HashMap::new(),
                pending_presented_window_ids: HashSet::new(),
                frame_debug_history: VecDeque::new(),
                next_frame_debug_index: 0,
                last_snapshot_capture_window_ids: HashSet::new(),
                last_snapshot_used_window_ids: HashSet::new(),
                last_live_window_ids: HashSet::new(),
                last_newly_presented_window_ids: HashSet::new(),
                backend: None,
                winit: None,
                stopped: false,
            }
        }

        fn test_bootstrap(socket_name: &str) -> TestBootstrap {
            test_bootstrap_with_state(socket_name, test_state_snapshot())
        }

        fn test_bootstrap_with_state(socket_name: &str, state: StateSnapshot) -> TestBootstrap {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime(socket_name);
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some(socket_name.into()),
            };

            SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            }
        }

        fn assert_topology_matches_known_surfaces(snapshot: &SmithayBootstrapSnapshot) {
            let runtime_state = &snapshot.runtime.state;
            let topology = &snapshot.topology;

            assert_eq!(
                topology.surfaces.len(),
                runtime_state.known_surfaces.all.len()
            );

            for toplevel in &runtime_state.known_surfaces.toplevels {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == toplevel.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Window);
                assert_eq!(surface.window_id.as_ref(), Some(&toplevel.window_id));
                assert_eq!(surface.parent_surface_id, None);
                assert!(surface.mapped);
                assert_eq!(surface.layer_metadata, None);
                assert!(
                    toplevel.requests.last_resize_serial.is_none()
                        || toplevel.requests.last_resize_edge.is_some()
                );
            }

            for popup in &runtime_state.known_surfaces.popups {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == popup.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Popup);
                assert!(surface.window_id.is_none());
                assert!(surface.mapped);
                assert!(popup.configure.pending_configure_count <= 1);
                if popup.configure.grab_requested {
                    assert!(popup.configure.last_grab_serial.is_some());
                }

                match &popup.parent {
                    crate::smithay_state::SmithayPopupParentSnapshot::Resolved {
                        surface_id,
                        ..
                    } => {
                        assert_eq!(
                            surface.parent_surface_id.as_deref(),
                            Some(surface_id.as_str())
                        );
                    }
                    crate::smithay_state::SmithayPopupParentSnapshot::Unresolved => {
                        assert_eq!(
                            surface.parent_surface_id.as_deref(),
                            Some(format!("unresolved-parent-{}", popup.surface_id).as_str())
                        );
                    }
                }
            }

            for unmanaged in &runtime_state.known_surfaces.unmanaged {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == unmanaged.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Unmanaged);
                assert!(surface.window_id.is_none());
                assert_eq!(surface.parent_surface_id, None);
                assert!(surface.mapped);
            }

            for layer in &runtime_state.known_surfaces.layers {
                let surface = topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == layer.surface_id)
                    .unwrap();
                assert_eq!(surface.role, SurfaceRole::Layer);
                assert_eq!(surface.output_id, layer.output_id);
                assert_eq!(surface.layer_metadata.as_ref(), Some(&layer.metadata));
                assert!(surface.window_id.is_none());
                assert_eq!(surface.parent_surface_id, None);
                assert!(surface.mapped);
            }
        }

        fn assert_output_summary_matches_topology(snapshot: &SmithayBootstrapSnapshot) {
            let runtime_state = &snapshot.runtime.state;
            let topology = &snapshot.topology;

            assert!(topology.outputs.len() >= runtime_state.outputs.known_output_ids.len());
            for output_id in &runtime_state.outputs.known_output_ids {
                assert!(
                    topology
                        .outputs
                        .iter()
                        .any(|output| output.snapshot.id == *output_id)
                );
            }
            if let Some(active_output_id) = runtime_state.outputs.active_output_id.as_ref() {
                assert_eq!(topology.active_output_id.as_ref(), Some(active_output_id));
            }
            assert_eq!(
                runtime_state.outputs.mapped_surface_count,
                topology
                    .surfaces
                    .iter()
                    .filter(|surface| surface.mapped)
                    .count()
            );

            let topology_active_output_attached_surface_count = runtime_state
                .outputs
                .active_output_id
                .as_ref()
                .map(|active_output_id| {
                    topology
                        .surfaces
                        .iter()
                        .filter(|surface| {
                            surface.mapped && surface.output_id.as_ref() == Some(active_output_id)
                        })
                        .count()
                })
                .unwrap_or(0);
            assert_eq!(
                runtime_state.outputs.active_output_attached_surface_count,
                topology_active_output_attached_surface_count
            );
        }

        fn assert_seat_summary_matches_topology(snapshot: &SmithayBootstrapSnapshot) {
            let runtime_state = &snapshot.runtime.state;
            let topology = &snapshot.topology;

            if let Some(seat) = topology
                .seats
                .iter()
                .find(|seat| seat.name == runtime_state.seat.name)
            {
                assert_eq!(
                    topology.active_seat_name.as_deref(),
                    Some(seat.name.as_str())
                );
                assert!(seat.active);
                assert_eq!(runtime_state.seat.focused_window_id, seat.focused_window_id);
                if let Some(focused_output_id) = runtime_state.seat.focused_output_id.as_ref() {
                    assert_eq!(seat.focused_output_id.as_ref(), Some(focused_output_id));
                }
            }
        }

        #[test]
        fn runtime_snapshot_exposes_state_snapshot() {
            let mut runtime = test_runtime("wayland-test-1");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Unmanaged {
                    surface_id: "wl-surface-1".into(),
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.socket_name, "wayland-test-1");
            assert_eq!(snapshot.window_size, (1280, 720));
            assert_eq!(snapshot.state.seat_name, "smithay-test-seat");
            assert_eq!(snapshot.state.seat.name, "smithay-test-seat");
            assert!(snapshot.state.seat.has_keyboard);
            assert!(snapshot.state.seat.has_pointer);
            assert!(!snapshot.state.seat.has_touch);
            assert!(snapshot.state.seat.focused_surface_id.is_none());
            assert!(snapshot.state.seat.focused_surface_role.is_none());
            assert!(snapshot.state.seat.focused_window_id.is_none());
            assert!(snapshot.state.seat.focused_output_id.is_none());
            assert_eq!(snapshot.state.seat.cursor_image, "default");
            assert!(snapshot.state.seat.cursor_surface_id.is_none());
            assert!(snapshot.state.outputs.known_output_ids.is_empty());
            assert!(snapshot.state.outputs.active_output_id.is_none());
            assert_eq!(snapshot.state.outputs.layer_surface_output_count, 0);
            assert_eq!(
                snapshot.state.outputs.active_output_attached_surface_count,
                0
            );
            assert_eq!(snapshot.state.outputs.mapped_surface_count, 1);
            assert_eq!(snapshot.state.tracked_surface_count, 1);
            assert_eq!(snapshot.state.role_counts.unmanaged, 1);
            assert_eq!(snapshot.state.known_surfaces.unmanaged.len(), 1);
            assert_eq!(snapshot.state.clipboard_selection.target, "clipboard");
            assert!(snapshot.state.selection_protocols.data_device);
            assert!(snapshot.state.selection_protocols.primary_selection);
            assert!(snapshot.state.selection_protocols.wlr_data_control);
            assert!(snapshot.state.selection_protocols.ext_data_control);
            assert!(snapshot.state.clipboard_selection.selection.is_none());
            assert!(
                snapshot
                    .state
                    .clipboard_selection
                    .focused_client_id
                    .is_none()
            );
            assert_eq!(snapshot.state.primary_selection.target, "primary");
            assert!(snapshot.state.primary_selection.selection.is_none());
            assert!(snapshot.state.primary_selection.focused_client_id.is_none());
        }

        #[test]
        fn bootstrap_snapshot_matches_runtime_snapshot() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-2");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-2".into()),
            };
            let bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.runtime, bootstrap.runtime.snapshot());
            assert_eq!(snapshot.runtime.socket_name, "wayland-test-2");
            assert_eq!(snapshot.controller, bootstrap.controller.report());
            assert_eq!(bootstrap.report.seat_name, "smithay-test-seat");
            assert_eq!(bootstrap.controller.phase(), ControllerPhase::Pending);
        }

        #[test]
        fn bootstrap_applies_pending_discovery_events_to_controller() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let mut runtime = test_runtime("wayland-test-3");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-601".into(),
                    window_id: spiders_shared::ids::WindowId::from("smithay-window-601"),
                    output_id: None,
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-3".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            let applied = bootstrap.apply_pending_discovery_events().unwrap();

            let snapshot = bootstrap.snapshot();
            assert_eq!(applied, 1);
            assert_eq!(snapshot.runtime.state.pending_discovery_event_count, 0);
            assert_eq!(snapshot.runtime.state.known_surfaces.toplevels.len(), 1);
            assert_eq!(snapshot.topology_surface_count, 1);
            assert_eq!(bootstrap.controller.phase(), ControllerPhase::Running);
            let surface = bootstrap
                .controller
                .app()
                .session()
                .topology()
                .surface("wl-surface-601")
                .unwrap();
            assert_eq!(surface.id, "wl-surface-601");
            assert_eq!(
                surface.window_id,
                Some(spiders_shared::ids::WindowId::from("smithay-window-601"))
            );
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].configure,
                crate::smithay_state::SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: false,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 0,
                }
            );
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].metadata,
                crate::smithay_state::SmithayXdgToplevelMetadataSnapshot {
                    title: None,
                    app_id: None,
                    parent_surface_id: None,
                    min_size: None,
                    max_size: None,
                    window_geometry: None,
                }
            );
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].requests,
                crate::smithay_state::SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: None,
                    last_resize_serial: None,
                    last_resize_edge: None,
                    last_window_menu_serial: None,
                    last_window_menu_location: None,
                    minimize_requested: false,
                    close_requested: false,
                    last_request_kind: None,
                    request_count: 0,
                }
            );
            assert_topology_matches_known_surfaces(&snapshot);
            assert_output_summary_matches_topology(&snapshot);
            assert_seat_summary_matches_topology(&snapshot);
        }

        #[test]
        fn runtime_drains_pending_discovery_events_as_controller_commands() {
            let mut runtime = test_runtime("wayland-test-discovery-drain");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);
            runtime
                .state_mut()
                .activate_output_id(OutputId::from("out-2"));
            runtime
                .state_mut()
                .remove_output_id(&OutputId::from("out-2"));

            let commands = runtime.drain_pending_discovery_commands();

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                &commands[0],
                ControllerCommand::DiscoveryEvent(
                    crate::backend::BackendDiscoveryEvent::OutputLost { output_id }
                ) if output_id == &OutputId::from("out-2")
            ));
        }

        #[test]
        fn bootstrap_applies_adapter_output_lifecycle_events_to_controller() {
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let mut bootstrap =
                test_bootstrap_with_state("wayland-test-adapter-output-lifecycle", state);

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::OutputActivated {
                    output_id: "out-2".into(),
                })
                .unwrap();
            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-2"))
            );

            bootstrap
                .apply_adapter_surface_discovery_batch(
                    1,
                    vec![crate::backend::BackendSurfaceSnapshot::Layer {
                        surface_id: "wl-adapter-output-layer-1".into(),
                        output_id: OutputId::from("out-2"),
                        metadata: LayerSurfaceMetadata {
                            namespace: "panel".into(),
                            tier: LayerSurfaceTier::Top,
                            keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                            exclusive_zone: LayerExclusiveZone::Exclusive(10),
                        },
                    }],
                )
                .unwrap();

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::OutputLost {
                    output_id: "out-2".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(
                snapshot
                    .topology
                    .outputs
                    .iter()
                    .all(|output| output.snapshot.id != OutputId::from("out-2"))
            );
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot
                    .topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == "wl-adapter-output-layer-1")
                    .unwrap()
                    .output_id,
                None
            );
        }

        #[test]
        fn bootstrap_applies_adapter_seat_lifecycle_and_focus_events_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-seat-lifecycle");

            bootstrap
                .apply_adapter_discovery_batch(
                    1,
                    vec![crate::backend::BackendSeatSnapshot {
                        seat_name: "seat-adapter".into(),
                        active: true,
                    }],
                    Vec::new(),
                    Vec::new(),
                )
                .unwrap();
            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SeatFocusChanged {
                    seat_name: "seat-adapter".into(),
                    window_id: Some("w1".into()),
                    output_id: Some("out-1".into()),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let seat = snapshot
                .topology
                .seats
                .iter()
                .find(|seat| seat.name == "seat-adapter")
                .unwrap();
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("seat-adapter")
            );
            assert_eq!(seat.focused_window_id, Some(WindowId::from("w1")));
            assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SeatLost {
                    seat_name: "seat-adapter".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(
                snapshot
                    .topology
                    .seats
                    .iter()
                    .all(|seat| seat.name != "seat-adapter")
            );
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("seat-0")
            );
        }

        #[test]
        fn bootstrap_applies_pending_smithay_seat_lifecycle_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-smithay-seat-lifecycle");

            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .runtime
                .state_mut()
                .register_seat_name("seat-extra", false);
            bootstrap
                .runtime
                .state_mut()
                .activate_seat_name("seat-extra");

            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 2);
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("seat-extra")
            );
            assert!(
                snapshot
                    .topology
                    .seats
                    .iter()
                    .any(|seat| seat.name == "seat-extra" && seat.active)
            );

            bootstrap.runtime.state_mut().remove_seat_name("seat-extra");
            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("seat-0")
            );
            assert!(
                snapshot
                    .topology
                    .seats
                    .iter()
                    .all(|seat| seat.name != "seat-extra")
            );
        }

        #[test]
        fn bootstrap_applies_adapter_output_discovery_batch_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-output-discovery");

            bootstrap
                .apply_adapter_discovery_batch(
                    1,
                    Vec::new(),
                    vec![crate::backend::BackendOutputSnapshot {
                        snapshot: OutputSnapshot {
                            id: OutputId::from("out-3"),
                            name: "DP-2".into(),
                            logical_x: 0,
                            logical_y: 0,
                            logical_width: 3440,
                            logical_height: 1440,
                            scale: 1,
                            transform: OutputTransform::Normal,
                            enabled: true,
                            current_workspace_id: None,
                        },
                        active: true,
                    }],
                    Vec::new(),
                )
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(
                snapshot
                    .topology
                    .outputs
                    .iter()
                    .any(|output| output.snapshot.id == OutputId::from("out-3"))
            );
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-3"))
            );
        }

        #[test]
        fn bootstrap_applies_adapter_output_snapshot_event_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-output-event");

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::OutputSnapshot {
                    output_id: "out-9".into(),
                    active: true,
                    x: 320,
                    y: 0,
                    width: 3840,
                    height: 2160,
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let output = snapshot
                .topology
                .outputs
                .iter()
                .find(|output| output.snapshot.id == OutputId::from("out-9"))
                .unwrap();
            assert_eq!(output.snapshot.name, "out-9");
            assert_eq!(output.snapshot.logical_width, 3840);
            assert_eq!(output.snapshot.logical_height, 2160);
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-9"))
            );
        }

        #[test]
        fn smithay_initial_winit_discovery_command_uses_typed_output_snapshot() {
            let command = super::initial_winit_discovery_command(
                "smithay-winit",
                "smithay-winit-output",
                (1280, 720),
                1.0,
            );

            let ControllerCommand::DiscoverySnapshot(snapshot) = command else {
                panic!("expected discovery snapshot command");
            };

            assert_eq!(snapshot.seats.len(), 1);
            assert_eq!(snapshot.outputs.len(), 1);
            assert_eq!(
                snapshot.outputs[0].snapshot.id,
                OutputId::from("smithay-winit-output")
            );
            assert_eq!(snapshot.outputs[0].snapshot.name, "smithay-winit-output");
            assert_eq!(snapshot.outputs[0].snapshot.logical_width, 1280);
            assert_eq!(snapshot.outputs[0].snapshot.logical_height, 720);
            assert!(snapshot.outputs[0].active);
        }

        #[test]
        fn smithay_initial_winit_seat_descriptor_marks_active_seat() {
            let descriptor = super::initial_winit_seat_descriptor("smithay-winit");

            assert_eq!(descriptor.seat_name, "smithay-winit");
            assert!(descriptor.active);
        }

        #[test]
        fn smithay_output_snapshot_matches_state_output_registration_metadata() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "smithay-winit").unwrap();
            let output = super::smithay_output_snapshot("smithay-winit-output", (1280, 720), 1.0);
            let smithay_output = Output::new(
                output.name.clone(),
                PhysicalProperties {
                    size: (1280, 720).into(),
                    subpixel: Subpixel::Unknown,
                    make: "Spiders".into(),
                    model: "Winit".into(),
                    serial_number: "Test".into(),
                },
            );

            state.register_smithay_output(
                output.id.clone(),
                smithay_output,
                Some((output.logical_x, output.logical_y)),
                Some((output.logical_width, output.logical_height)),
                true,
            );

            let snapshot = state.snapshot();
            assert_eq!(snapshot.outputs.known_outputs.len(), 1);
            assert_eq!(snapshot.outputs.known_outputs[0].id, output.id);
            assert_eq!(snapshot.outputs.known_outputs[0].name, output.name);
            assert_eq!(
                snapshot.outputs.known_outputs[0].logical_width,
                Some(output.logical_width)
            );
            assert_eq!(
                snapshot.outputs.known_outputs[0].logical_height,
                Some(output.logical_height)
            );
            assert_eq!(
                snapshot.outputs.active_output_id,
                Some(OutputId::from("smithay-winit-output"))
            );
        }

        #[test]
        fn smithay_state_extracts_backend_surface_snapshots_from_tracked_surfaces() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_id(OutputId::from("out-1"), true);
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Window {
                surface_id: "wl-window-extract-1".into(),
                window_id: WindowId::from("w1"),
                output_id: None,
            });
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Popup {
                surface_id: "wl-popup-extract-1".into(),
                output_id: Some(OutputId::from("out-1")),
                parent_surface_id: "wl-window-extract-1".into(),
            });
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Layer {
                surface_id: "wl-layer-extract-1".into(),
                output_id: OutputId::from("out-1"),
                metadata: LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(8),
                },
            });
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Unmanaged {
                surface_id: "wl-unmanaged-extract-1".into(),
            });

            let snapshots = state.backend_surface_snapshots();
            assert_eq!(snapshots.len(), 4);
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Window { surface_id, .. } if surface_id == "wl-window-extract-1")
            }));
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Popup { surface_id, .. } if surface_id == "wl-popup-extract-1")
            }));
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Layer { surface_id, .. } if surface_id == "wl-layer-extract-1")
            }));
            assert!(snapshots.iter().any(|snapshot| {
                matches!(snapshot, crate::backend::BackendSurfaceSnapshot::Unmanaged { surface_id } if surface_id == "wl-unmanaged-extract-1")
            }));
        }

        #[test]
        fn bootstrap_applies_tracked_smithay_surface_discovery_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-tracked-surface-discovery");
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-bootstrap-window-1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: None,
                },
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-bootstrap-popup-1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    parent_surface_id: "wl-bootstrap-window-1".into(),
                },
            );

            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_surface_discovery(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(
                snapshot
                    .topology
                    .surfaces
                    .iter()
                    .any(|surface| surface.id == "wl-bootstrap-window-1"
                        && surface.role == SurfaceRole::Window)
            );
            assert!(
                snapshot
                    .topology
                    .surfaces
                    .iter()
                    .any(|surface| surface.id == "wl-bootstrap-popup-1"
                        && surface.role == SurfaceRole::Popup)
            );
        }

        #[test]
        fn smithay_state_extracts_backend_topology_snapshot_from_known_state() {
            let display = Display::<SpidersSmithayState>::new().unwrap();
            let mut state = SpidersSmithayState::new(&display, "test-seat").unwrap();
            state.register_output_snapshot(
                OutputId::from("out-topology-1"),
                "DP-1",
                Some((0, 0)),
                Some((2560, 1440)),
                true,
            );
            state.track_test_surface_snapshot(crate::backend::BackendSurfaceSnapshot::Window {
                surface_id: "wl-topology-window-1".into(),
                window_id: WindowId::from("w1"),
                output_id: None,
            });

            let snapshot = state.backend_topology_snapshot(7);
            assert_eq!(snapshot.source, crate::backend::BackendSource::Smithay);
            assert_eq!(snapshot.generation, 7);
            assert_eq!(snapshot.seats.len(), 1);
            assert_eq!(snapshot.outputs.len(), 1);
            assert_eq!(
                snapshot.outputs[0].snapshot.id,
                OutputId::from("out-topology-1")
            );
            assert_eq!(snapshot.outputs[0].snapshot.name, "DP-1");
            assert!(snapshot.outputs[0].active);
            assert_eq!(snapshot.surfaces.len(), 1);
        }

        #[test]
        fn bootstrap_applies_tracked_smithay_discovery_snapshot_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-tracked-discovery-snapshot");
            bootstrap.runtime.state_mut().register_output_snapshot(
                OutputId::from("out-snapshot-1"),
                "HDMI-A-1",
                Some((0, 0)),
                Some((1920, 1080)),
                true,
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-snapshot-window-1".into(),
                    window_id: WindowId::from("w1"),
                    output_id: Some(OutputId::from("out-snapshot-1")),
                },
            );

            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(9)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(
                snapshot
                    .topology
                    .outputs
                    .iter()
                    .any(|output| output.snapshot.id == OutputId::from("out-snapshot-1"))
            );
            assert!(
                snapshot
                    .topology
                    .surfaces
                    .iter()
                    .any(|surface| surface.id == "wl-snapshot-window-1"
                        && surface.role == SurfaceRole::Window)
            );
            assert_eq!(
                snapshot
                    .controller
                    .backend
                    .as_ref()
                    .and_then(|backend| backend.last_generation),
                Some(9)
            );
        }

        #[test]
        fn bootstrap_applies_adapter_surface_unmap_and_loss_events_to_controller() {
            let mut bootstrap = test_bootstrap("wayland-test-adapter-surface-lifecycle");

            bootstrap
                .apply_adapter_surface_discovery_batch(
                    1,
                    vec![
                        crate::backend::BackendSurfaceSnapshot::Window {
                            surface_id: "wl-adapter-window-1".into(),
                            window_id: WindowId::from("w1"),
                            output_id: Some(OutputId::from("out-1")),
                        },
                        crate::backend::BackendSurfaceSnapshot::Popup {
                            surface_id: "wl-adapter-popup-1".into(),
                            output_id: Some(OutputId::from("out-1")),
                            parent_surface_id: "wl-adapter-window-1".into(),
                        },
                    ],
                )
                .unwrap();

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SurfaceUnmapped {
                    surface_id: "wl-adapter-window-1".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let window = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-adapter-window-1")
                .unwrap();
            let popup = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-adapter-popup-1")
                .unwrap();
            assert!(!window.mapped);
            assert!(!popup.mapped);

            bootstrap
                .apply_adapter_event(SmithayAdapterEvent::SurfaceLost {
                    surface_id: "wl-adapter-window-1".into(),
                })
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert!(snapshot.topology.surfaces.iter().all(|surface| {
                surface.id != "wl-adapter-window-1" && surface.id != "wl-adapter-popup-1"
            }));
        }

        #[test]
        fn bootstrap_applies_batched_adapter_lifecycle_events_to_controller() {
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let mut bootstrap = test_bootstrap_with_state("wayland-test-adapter-batch", state);

            bootstrap
                .apply_adapter_surface_discovery_batch(
                    1,
                    vec![crate::backend::BackendSurfaceSnapshot::Window {
                        surface_id: "wl-batch-window-1".into(),
                        window_id: WindowId::from("w1"),
                        output_id: Some(OutputId::from("out-2")),
                    }],
                )
                .unwrap();

            let applied = bootstrap
                .apply_adapter_events(vec![
                    SmithayAdapterEvent::Seat {
                        seat_name: "seat-batch".into(),
                        active: true,
                    },
                    SmithayAdapterEvent::SeatFocusChanged {
                        seat_name: "seat-batch".into(),
                        window_id: Some("w1".into()),
                        output_id: Some("out-2".into()),
                    },
                    SmithayAdapterEvent::OutputActivated {
                        output_id: "out-2".into(),
                    },
                    SmithayAdapterEvent::SurfaceUnmapped {
                        surface_id: "wl-batch-window-1".into(),
                    },
                ])
                .unwrap();

            assert_eq!(applied, 4);

            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("seat-batch")
            );
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-2"))
            );
            let seat = snapshot
                .topology
                .seats
                .iter()
                .find(|seat| seat.name == "seat-batch")
                .unwrap();
            assert_eq!(seat.focused_window_id, None);
            assert_eq!(seat.focused_output_id, Some(OutputId::from("out-2")));
            let surface = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-batch-window-1")
                .unwrap();
            assert!(!surface.mapped);
        }

        #[test]
        fn bootstrap_snapshot_exposes_rich_topology_for_mixed_surface_roles() {
            let mut bootstrap = test_bootstrap("wayland-test-5");

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-701".into(),
                    window_id: WindowId::from("smithay-window-701"),
                    output_id: None,
                },
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-surface-702".into(),
                    output_id: None,
                    parent_surface_id: "wl-surface-701".into(),
                },
            );
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Unmanaged {
                    surface_id: "wl-surface-703".into(),
                },
            );
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-surface-704".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.topology_surface_count, 4);
            assert_eq!(snapshot.topology.surfaces.len(), 4);
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.topology.active_seat_name.as_deref(),
                Some("smithay-test-seat")
            );
            assert_topology_matches_known_surfaces(&snapshot);
            assert_output_summary_matches_topology(&snapshot);
            assert_seat_summary_matches_topology(&snapshot);

            let popup = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-702")
                .unwrap();
            assert_eq!(popup.parent_surface_id.as_deref(), Some("wl-surface-701"));
            assert_eq!(popup.role, SurfaceRole::Popup);

            let layer = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-704")
                .unwrap();
            assert_eq!(layer.role, SurfaceRole::Layer);
            assert_eq!(layer.output_id, Some(OutputId::from("out-1")));
            assert_eq!(
                layer.layer_metadata,
                Some(LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                })
            );
        }

        #[test]
        fn runtime_snapshot_exposes_known_layer_surface_output_attachment() {
            let mut runtime = test_runtime("wayland-test-layer-1");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "background".into(),
                        tier: LayerSurfaceTier::Background,
                        keyboard_interactivity: LayerKeyboardInteractivity::None,
                        exclusive_zone: LayerExclusiveZone::Neutral,
                    },
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.role_counts.layer, 1);
            assert_eq!(snapshot.state.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].metadata,
                LayerSurfaceMetadata {
                    namespace: "background".into(),
                    tier: LayerSurfaceTier::Background,
                    keyboard_interactivity: LayerKeyboardInteractivity::None,
                    exclusive_zone: LayerExclusiveZone::Neutral,
                }
            );
            assert_eq!(
                snapshot.state.known_surfaces.layers[0]
                    .configure
                    .last_acked_serial,
                None
            );
            assert_eq!(
                snapshot.state.known_surfaces.layers[0]
                    .configure
                    .last_configured_size,
                None
            );
        }

        #[test]
        fn runtime_snapshot_exposes_layer_configure_inspection() {
            let mut runtime = test_runtime("wayland-test-layer-configure-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-configure-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            runtime.state_mut().set_test_layer_configure_snapshot(
                "wl-layer-configure-1",
                crate::smithay_state::SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(99),
                    pending_configure_count: 0,
                    last_configured_size: Some((1280, 36)),
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].configure,
                crate::smithay_state::SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: Some(99),
                    pending_configure_count: 0,
                    last_configured_size: Some((1280, 36)),
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_layer_pending_configure_counts() {
            let mut runtime = test_runtime("wayland-test-layer-configure-2");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-configure-2".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(18),
                    },
                },
            );
            runtime
                .state_mut()
                .record_test_layer_configure_sent("wl-layer-configure-2", Some((1024, 30)));
            runtime
                .state_mut()
                .record_test_layer_configure_sent("wl-layer-configure-2", Some((1024, 32)));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.layers.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.layers[0].configure,
                crate::smithay_state::SmithayLayerSurfaceConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 2,
                    last_configured_size: Some((1024, 32)),
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_explicit_layer_parented_popup_tracking() {
            let mut runtime = test_runtime("wayland-test-layer-popup-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-parent-1".into(),
                    output_id: OutputId::from("out-7"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(10),
                    },
                },
            );
            let _ = runtime.state_mut().take_discovery_events();
            runtime
                .state_mut()
                .track_layer_popup_surface_for_test("wl-layer-parent-1", "wl-popup-child-1");

            let snapshot = runtime.snapshot();
            let popup = snapshot
                .state
                .known_surfaces
                .popups
                .iter()
                .find(|popup| popup.surface_id == "wl-popup-child-1")
                .unwrap();
            assert_eq!(
                popup.parent,
                crate::smithay_state::SmithayPopupParentSnapshot::Resolved {
                    surface_id: "wl-layer-parent-1".into(),
                    window_id: None,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_popup_pending_configure_counts() {
            let mut runtime = test_runtime("wayland-test-popup-configure-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-configure-1".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-configure-1".into(),
                },
            );
            runtime.state_mut().record_test_xdg_popup_configure_sent(
                "wl-popup-configure-1",
                Some(31),
                true,
                (12, 14, 240, 160),
            );
            runtime.state_mut().record_test_xdg_popup_configure_sent(
                "wl-popup-configure-1",
                Some(32),
                true,
                (12, 14, 260, 180),
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 2,
                    last_reposition_token: Some(32),
                    reactive: true,
                    geometry: (12, 14, 260, 180),
                    last_grab_serial: None,
                    grab_requested: false,
                    last_request_kind: Some("reposition".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_initial_popup_pending_configure() {
            let mut runtime = test_runtime("wayland-test-popup-configure-init");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-configure-init".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-configure-init".into(),
                },
            );
            runtime
                .state_mut()
                .record_test_initial_xdg_popup_configure_sent(
                    "wl-popup-configure-init",
                    false,
                    (6, 8, 190, 120),
                );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.popups.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 1,
                    last_reposition_token: None,
                    reactive: false,
                    geometry: (6, 8, 190, 120),
                    last_grab_serial: None,
                    grab_requested: false,
                    last_request_kind: None,
                    request_count: 0,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_popup_request_sequence() {
            let mut runtime = test_runtime("wayland-test-popup-request-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-request-1".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-request-1".into(),
                },
            );
            runtime.state_mut().record_test_xdg_popup_request(
                "wl-popup-request-1",
                "grab",
                |snapshot| {
                    snapshot.last_grab_serial = Some(51);
                    snapshot.grab_requested = true;
                },
            );
            runtime.state_mut().record_test_xdg_popup_request(
                "wl-popup-request-1",
                "reposition",
                |snapshot| {
                    snapshot.last_reposition_token = Some(52);
                    snapshot.reactive = true;
                    snapshot.geometry = (8, 9, 180, 120);
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: None,
                    pending_configure_count: 0,
                    last_reposition_token: Some(52),
                    reactive: true,
                    geometry: (8, 9, 180, 120),
                    last_grab_serial: Some(51),
                    grab_requested: true,
                    last_request_kind: Some("reposition".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_toplevel_pending_configure_counts() {
            let mut runtime = test_runtime("wayland-test-toplevel-configure-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-toplevel-configure-1".into(),
                    window_id: WindowId::from("smithay-window-top-1"),
                    output_id: None,
                },
            );
            runtime.state_mut().record_test_xdg_toplevel_configure_sent(
                "wl-toplevel-configure-1",
                true,
                false,
                true,
            );
            runtime.state_mut().record_test_xdg_toplevel_configure_sent(
                "wl-toplevel-configure-1",
                true,
                false,
                false,
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.toplevels[0].configure,
                crate::smithay_state::SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: true,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 2,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_initial_toplevel_pending_configure() {
            let mut runtime = test_runtime("wayland-test-toplevel-configure-init");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-toplevel-configure-init".into(),
                    window_id: WindowId::from("smithay-window-top-init"),
                    output_id: None,
                },
            );
            runtime.state_mut().record_test_xdg_toplevel_configure_sent(
                "wl-toplevel-configure-init",
                true,
                false,
                false,
            );

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.known_surfaces.toplevels.len(), 1);
            assert_eq!(
                snapshot.state.known_surfaces.toplevels[0].configure,
                crate::smithay_state::SmithayXdgToplevelConfigureSnapshot {
                    last_acked_serial: None,
                    activated: true,
                    fullscreen: false,
                    maximized: false,
                    pending_configure_count: 1,
                }
            );
        }

        #[test]
        fn runtime_snapshot_exposes_toplevel_buffer_presence() {
            let mut runtime = test_runtime("wayland-test-toplevel-buffer-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-toplevel-buffer-1".into(),
                    window_id: WindowId::from("smithay-window-buffer-1"),
                    output_id: None,
                },
            );

            let snapshot = runtime.snapshot();
            assert!(snapshot.state.known_surfaces.toplevels[0].has_buffer);

            runtime
                .state_mut()
                .set_test_surface_has_buffer("wl-toplevel-buffer-1", false);
            let snapshot = runtime.snapshot();
            assert!(!snapshot.state.known_surfaces.toplevels[0].has_buffer);
        }

        #[test]
        fn runtime_snapshot_exposes_xdg_toplevel_request_sequence() {
            let mut runtime = test_runtime("wayland-test-toplevel-request-1");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-toplevel-request-1".into(),
                    window_id: WindowId::from("smithay-window-request-1"),
                    output_id: None,
                },
            );
            runtime.state_mut().set_test_toplevel_request_snapshot(
                "wl-toplevel-request-1",
                crate::smithay_state::SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: Some(41),
                    last_resize_serial: None,
                    last_resize_edge: None,
                    last_window_menu_serial: None,
                    last_window_menu_location: None,
                    minimize_requested: true,
                    close_requested: false,
                    last_request_kind: Some("minimize".into()),
                    request_count: 2,
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.known_surfaces.toplevels[0].requests,
                crate::smithay_state::SmithayXdgToplevelRequestSnapshot {
                    last_move_serial: Some(41),
                    last_resize_serial: None,
                    last_resize_edge: None,
                    last_window_menu_serial: None,
                    last_window_menu_location: None,
                    minimize_requested: true,
                    close_requested: false,
                    last_request_kind: Some("minimize".into()),
                    request_count: 2,
                }
            );
        }

        #[test]
        fn smithay_bootstrap_preserves_layer_keyboard_and_exclusive_zone_metadata() {
            let mut bootstrap = test_bootstrap("wayland-test-layer-meta-1");
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-meta-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "lockscreen".into(),
                        tier: LayerSurfaceTier::Overlay,
                        keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                        exclusive_zone: LayerExclusiveZone::DontCare,
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let layer = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-layer-meta-1")
                .unwrap();
            assert_eq!(
                layer.layer_metadata,
                Some(LayerSurfaceMetadata {
                    namespace: "lockscreen".into(),
                    tier: LayerSurfaceTier::Overlay,
                    keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                    exclusive_zone: LayerExclusiveZone::DontCare,
                })
            );
            assert_topology_matches_known_surfaces(&snapshot);
            assert_output_summary_matches_topology(&snapshot);
            assert_seat_summary_matches_topology(&snapshot);
        }

        #[test]
        fn bootstrap_unmaps_and_remaps_layer_surface_without_losing_output_attachment() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let mut runtime = test_runtime("wayland-test-layer-2");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-2".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-layer-2".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_unmap("wl-layer-2");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let unmapped = bootstrap.snapshot();
            let layer = unmapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-layer-2")
                .unwrap();
            assert_eq!(layer.role, SurfaceRole::Layer);
            assert_eq!(layer.output_id, Some(OutputId::from("out-1")));
            assert!(!layer.mapped);

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-2".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let remapped = bootstrap.snapshot();
            let layer = remapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-layer-2")
                .unwrap();
            assert_eq!(layer.output_id, Some(OutputId::from("out-1")));
            assert_eq!(
                layer.layer_metadata,
                Some(LayerSurfaceMetadata {
                    namespace: "panel".into(),
                    tier: LayerSurfaceTier::Top,
                    keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                    exclusive_zone: LayerExclusiveZone::Exclusive(20),
                })
            );
            assert!(layer.mapped);
            assert_topology_matches_known_surfaces(&remapped);
            assert_output_summary_matches_topology(&remapped);
            assert_seat_summary_matches_topology(&remapped);
        }

        #[test]
        fn bootstrap_removes_layer_surface_from_topology_when_smithay_layer_is_lost() {
            let mut bootstrap = test_bootstrap("wayland-test-layer-3");
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-3".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "overlay".into(),
                        tier: LayerSurfaceTier::Overlay,
                        keyboard_interactivity: LayerKeyboardInteractivity::Exclusive,
                        exclusive_zone: LayerExclusiveZone::DontCare,
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();
            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_loss("wl-layer-3");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert!(snapshot.runtime.state.known_surfaces.layers.is_empty());
            assert!(
                snapshot
                    .topology
                    .surfaces
                    .iter()
                    .all(|surface| surface.id != "wl-layer-3")
            );
        }

        #[test]
        fn bootstrap_removes_topology_surface_when_smithay_surface_is_lost() {
            let mut bootstrap = test_bootstrap("wayland-test-6");

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-801".into(),
                    window_id: WindowId::from("smithay-window-801"),
                    output_id: None,
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();
            assert!(
                bootstrap
                    .controller
                    .app()
                    .session()
                    .topology()
                    .surface("wl-surface-801")
                    .is_some()
            );

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_loss("wl-surface-801");

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.runtime.state.known_surfaces.toplevels.len(), 0);
            assert_eq!(snapshot.runtime.state.tracked_surface_count, 0);
            assert_eq!(snapshot.topology_surface_count, 0);
            assert!(snapshot.topology.surfaces.is_empty());
            assert!(
                bootstrap
                    .controller
                    .app()
                    .session()
                    .topology()
                    .surface("wl-surface-801")
                    .is_none()
            );
        }

        #[test]
        fn bootstrap_unmaps_and_remaps_topology_surface_when_smithay_surface_buffer_changes() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let mut runtime = test_runtime("wayland-test-7");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-901".into(),
                    window_id: WindowId::from("smithay-window-901"),
                    output_id: None,
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-7".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);
            assert!(
                bootstrap
                    .controller
                    .app()
                    .session()
                    .topology()
                    .surface("wl-surface-901")
                    .unwrap()
                    .mapped
            );

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_unmap("wl-surface-901");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let unmapped = bootstrap.snapshot();
            let surface = unmapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-901")
                .unwrap();
            assert!(!surface.mapped);
            assert!(
                unmapped
                    .topology
                    .outputs
                    .iter()
                    .find(|output| output.snapshot.id == OutputId::from("out-1"))
                    .unwrap()
                    .mapped_surface_ids
                    .is_empty()
            );
            assert_eq!(unmapped.runtime.state.known_surfaces.toplevels.len(), 1);

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-901".into(),
                    window_id: WindowId::from("smithay-window-901"),
                    output_id: None,
                },
            );
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let remapped = bootstrap.snapshot();
            let surface = remapped
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-surface-901")
                .unwrap();
            assert!(surface.mapped);
            assert_eq!(
                surface.window_id.as_ref(),
                Some(&WindowId::from("smithay-window-901"))
            );
            assert_topology_matches_known_surfaces(&remapped);
        }

        #[test]
        fn bootstrap_cascades_popup_unmap_and_removal_from_parent_surface() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let mut runtime = test_runtime("wayland-test-8");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-1001".into(),
                    window_id: WindowId::from("smithay-window-1001"),
                    output_id: None,
                },
            );
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-surface-1002".into(),
                    output_id: None,
                    parent_surface_id: "wl-surface-1001".into(),
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-8".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 2);

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_unmap("wl-surface-1001");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let unmapped = bootstrap.snapshot();
            assert!(
                !unmapped
                    .topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == "wl-surface-1001")
                    .unwrap()
                    .mapped
            );
            assert!(
                !unmapped
                    .topology
                    .surfaces
                    .iter()
                    .find(|surface| surface.id == "wl-surface-1002")
                    .unwrap()
                    .mapped
            );

            bootstrap
                .runtime
                .state_mut()
                .track_test_surface_loss("wl-surface-1001");
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let removed = bootstrap.snapshot();
            assert!(
                removed
                    .topology
                    .surfaces
                    .iter()
                    .all(|surface| surface.id != "wl-surface-1001")
            );
            assert!(
                removed
                    .topology
                    .surfaces
                    .iter()
                    .all(|surface| surface.id != "wl-surface-1002")
            );
        }

        #[test]
        fn bootstrap_preserves_output_for_popup_parented_to_layer_surface() {
            let mut bootstrap = test_bootstrap("wayland-test-9");

            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-51".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(20),
                    },
                },
            );
            bootstrap
                .runtime
                .state_mut()
                .track_test_popup_parent("wl-popup-51", "wl-layer-51");
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-51".into(),
                    output_id: Some(OutputId::from("out-1")),
                    parent_surface_id: "wl-layer-51".into(),
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .apply_tracked_smithay_discovery_snapshot(1)
                .unwrap();

            let snapshot = bootstrap.snapshot();
            let popup = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-popup-51")
                .unwrap();
            assert_eq!(popup.role, SurfaceRole::Popup);
            assert_eq!(popup.parent_surface_id.as_deref(), Some("wl-layer-51"));
            assert_eq!(popup.output_id, Some(OutputId::from("out-1")));
        }

        #[test]
        fn bootstrap_snapshot_preserves_xdg_popup_configure_metadata() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let mut runtime = test_runtime("wayland-test-popup-meta-1");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-meta-1".into(),
                    output_id: None,
                    parent_surface_id: "unresolved-parent-wl-popup-meta-1".into(),
                },
            );
            runtime.state_mut().set_test_popup_configure_snapshot(
                "wl-popup-meta-1",
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: Some(18),
                    pending_configure_count: 0,
                    last_reposition_token: Some(77),
                    reactive: true,
                    geometry: (15, 25, 320, 180),
                    last_grab_serial: Some(14),
                    grab_requested: true,
                    last_request_kind: Some("grab".into()),
                    request_count: 2,
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-popup-meta-1".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.runtime.state.known_surfaces.popups[0].configure,
                crate::smithay_state::SmithayXdgPopupConfigureSnapshot {
                    last_acked_serial: Some(18),
                    pending_configure_count: 0,
                    last_reposition_token: Some(77),
                    reactive: true,
                    geometry: (15, 25, 320, 180),
                    last_grab_serial: Some(14),
                    grab_requested: true,
                    last_request_kind: Some("grab".into()),
                    request_count: 2,
                }
            );
            assert_topology_matches_known_surfaces(&snapshot);
        }

        #[test]
        fn bootstrap_snapshot_preserves_xdg_toplevel_size_constraints() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let mut runtime = test_runtime("wayland-test-xdg-size-1");

            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-size-1".into(),
                    window_id: WindowId::from("smithay-window-size-1"),
                    output_id: None,
                },
            );
            runtime.state_mut().set_test_toplevel_metadata_snapshot(
                "wl-surface-size-1",
                crate::smithay_state::SmithayXdgToplevelMetadataSnapshot {
                    title: Some("settings".into()),
                    app_id: Some("spiders.settings".into()),
                    parent_surface_id: None,
                    min_size: Some((800, 600)),
                    max_size: Some((2560, 1440)),
                    window_geometry: Some((30, 40, 1440, 900)),
                },
            );

            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-xdg-size-1".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            let snapshot = bootstrap.snapshot();
            assert_eq!(
                snapshot.runtime.state.known_surfaces.toplevels[0].metadata,
                crate::smithay_state::SmithayXdgToplevelMetadataSnapshot {
                    title: Some("settings".into()),
                    app_id: Some("spiders.settings".into()),
                    parent_surface_id: None,
                    min_size: Some((800, 600)),
                    max_size: Some((2560, 1440)),
                    window_geometry: Some((30, 40, 1440, 900)),
                }
            );
            assert_topology_matches_known_surfaces(&snapshot);
        }

        #[test]
        fn runtime_snapshot_exposes_clipboard_selection_inspection() {
            let mut runtime = test_runtime("wayland-test-clipboard-1");
            runtime.state_mut().set_test_clipboard_selection(Some(
                crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "data-device".into(),
                },
            ));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.clipboard_selection.target, "clipboard");
            assert_eq!(
                snapshot.state.clipboard_selection.selection,
                Some(crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "data-device".into(),
                })
            );
            assert!(
                snapshot
                    .state
                    .clipboard_selection
                    .focused_client_id
                    .is_none()
            );
        }

        #[test]
        fn runtime_snapshot_exposes_clipboard_focus_inspection() {
            let mut runtime = test_runtime("wayland-test-clipboard-2");
            runtime
                .state_mut()
                .set_test_clipboard_focus_client_id(Some("client-9"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot
                    .state
                    .clipboard_selection
                    .focused_client_id
                    .as_deref(),
                Some("client-9")
            );
        }

        #[test]
        fn runtime_snapshot_exposes_primary_selection_inspection() {
            let mut runtime = test_runtime("wayland-test-primary-1");
            runtime.state_mut().set_test_primary_selection(Some(
                crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "primary-selection".into(),
                },
            ));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.primary_selection.target, "primary");
            assert_eq!(
                snapshot.state.primary_selection.selection,
                Some(crate::smithay_state::SmithaySelectionOfferSnapshot {
                    mime_types: vec!["text/plain".into()],
                    source_kind: "primary-selection".into(),
                })
            );
            assert!(snapshot.state.primary_selection.focused_client_id.is_none());
        }

        #[test]
        fn runtime_snapshot_exposes_primary_focus_inspection() {
            let mut runtime = test_runtime("wayland-test-primary-2");
            runtime
                .state_mut()
                .set_test_primary_focus_client_id(Some("client-13"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot
                    .state
                    .primary_selection
                    .focused_client_id
                    .as_deref(),
                Some("client-13")
            );
        }

        #[test]
        fn runtime_snapshot_exposes_selection_protocol_support() {
            let runtime = test_runtime("wayland-test-selection-support");

            let snapshot = runtime.snapshot();
            assert!(snapshot.state.selection_protocols.data_device);
            assert!(snapshot.state.selection_protocols.primary_selection);
            assert!(snapshot.state.selection_protocols.wlr_data_control);
            assert!(snapshot.state.selection_protocols.ext_data_control);
        }

        #[test]
        fn runtime_snapshot_exposes_seat_focus_inspection() {
            let mut runtime = test_runtime("wayland-test-seat-focus");
            runtime
                .state_mut()
                .set_test_focused_surface_id(Some("wl-surface-501"));

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.seat.name, "smithay-test-seat");
            assert_eq!(
                snapshot.state.seat.focused_surface_id.as_deref(),
                Some("wl-surface-501")
            );
        }

        #[test]
        fn runtime_snapshot_exposes_focused_role_and_window_summary() {
            let mut runtime = test_runtime("wayland-test-seat-focus-summary");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-601".into(),
                    window_id: WindowId::from("smithay-window-601"),
                    output_id: None,
                },
            );
            runtime
                .state_mut()
                .set_test_focused_surface_id(Some("wl-surface-601"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.seat.focused_surface_role.as_deref(),
                Some("toplevel")
            );
            assert_eq!(
                snapshot.state.seat.focused_window_id,
                Some(WindowId::from("smithay-window-601"))
            );
            assert!(snapshot.state.seat.focused_output_id.is_none());
        }

        #[test]
        fn runtime_snapshot_exposes_focused_output_summary() {
            let mut runtime = test_runtime("wayland-test-seat-focus-output");
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-focus-runtime-1".into(),
                    output_id: OutputId::from("out-5"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(8),
                    },
                },
            );
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Popup {
                    surface_id: "wl-popup-focus-runtime-1".into(),
                    output_id: Some(OutputId::from("out-5")),
                    parent_surface_id: "wl-layer-focus-runtime-1".into(),
                },
            );
            runtime
                .state_mut()
                .track_test_popup_parent("wl-popup-focus-runtime-1", "wl-layer-focus-runtime-1");
            runtime
                .state_mut()
                .set_test_focused_surface_id(Some("wl-popup-focus-runtime-1"));

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.seat.focused_surface_role.as_deref(),
                Some("popup")
            );
            assert_eq!(
                snapshot.state.seat.focused_output_id,
                Some(OutputId::from("out-5"))
            );
        }

        #[test]
        fn runtime_snapshot_exposes_cursor_inspection() {
            let mut runtime = test_runtime("wayland-test-seat-cursor");
            runtime
                .state_mut()
                .set_test_cursor_image("named:Crosshair", None);

            let snapshot = runtime.snapshot();
            assert_eq!(snapshot.state.seat.cursor_image, "named:Crosshair");
            assert!(snapshot.state.seat.cursor_surface_id.is_none());
        }

        #[test]
        fn runtime_snapshot_exposes_output_inspection() {
            let mut runtime = test_runtime("wayland-test-output-state");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.outputs.known_output_ids,
                vec![OutputId::from("out-1"), OutputId::from("out-2")]
            );
            assert_eq!(
                snapshot.state.outputs.active_output_id,
                Some(OutputId::from("out-1"))
            );
            assert_eq!(
                snapshot.state.outputs.active_output_attached_surface_count,
                0
            );
        }

        #[test]
        fn runtime_snapshot_exposes_output_attachment_summary() {
            let mut runtime = test_runtime("wayland-test-output-summary");
            runtime
                .state_mut()
                .register_output_id(OutputId::from("out-1"), true);
            runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-layer-summary-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(12),
                    },
                },
            );

            let snapshot = runtime.snapshot();
            assert_eq!(
                snapshot.state.outputs.active_output_attached_surface_count,
                1
            );
            assert_eq!(snapshot.state.outputs.mapped_surface_count, 1);
        }

        #[test]
        fn titlebar_text_truncates_when_width_is_limited() {
            let truncated = truncate_titlebar_text("very long terminal title", 1, 64);
            assert_eq!(truncated, "very ...");
        }

        #[test]
        fn titlebar_text_preserves_short_titles() {
            let truncated = truncate_titlebar_text("term", 1, 64);
            assert_eq!(truncated, "term");
        }

        #[test]
        fn bootstrap_apply_pending_discovery_events_returns_zero_when_empty() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-4");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-4".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            let applied = bootstrap.apply_pending_discovery_events().unwrap();

            assert_eq!(applied, 0);
            let snapshot = bootstrap.snapshot();
            assert_eq!(snapshot.runtime.state.pending_discovery_event_count, 0);
            assert_eq!(snapshot.topology_surface_count, 0);
            assert_eq!(bootstrap.controller.phase(), ControllerPhase::Pending);
        }

        #[test]
        fn bootstrap_applies_pending_seat_focus_discovery_events_to_controller() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let state = test_state_snapshot();
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-seat-focus-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-seat-focus-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-seat-focus-1".into(),
                    output_id: OutputId::from("out-1"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(8),
                    },
                },
            );
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .runtime
                .state_mut()
                .record_test_seat_focus_event(Some("wl-seat-focus-1"));

            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            let seat = snapshot
                .topology
                .seats
                .iter()
                .find(|seat| seat.name == "smithay-test-seat")
                .unwrap();
            assert_eq!(seat.focused_window_id, None);
            assert_eq!(seat.focused_output_id, Some(OutputId::from("out-1")));
        }

        #[test]
        fn bootstrap_applies_pending_output_activation_discovery_events_to_controller() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state.outputs.push(spiders_shared::wm::OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: spiders_shared::wm::OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-output-activate-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-output-activate-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };
            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);
            let _ = bootstrap.runtime.state_mut().take_discovery_events();
            bootstrap
                .runtime
                .state_mut()
                .activate_output_id(OutputId::from("out-2"));

            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 0);
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-1"))
            );
        }

        #[test]
        fn bootstrap_applies_pending_output_lost_discovery_events_to_controller() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-output-lost-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-output-lost-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            bootstrap
                .runtime
                .state_mut()
                .register_output_id(OutputId::from("out-2"), false);
            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Layer {
                    surface_id: "wl-output-lost-layer-1".into(),
                    output_id: OutputId::from("out-2"),
                    metadata: LayerSurfaceMetadata {
                        namespace: "panel".into(),
                        tier: LayerSurfaceTier::Top,
                        keyboard_interactivity: LayerKeyboardInteractivity::OnDemand,
                        exclusive_zone: LayerExclusiveZone::Exclusive(10),
                    },
                },
            );
            bootstrap
                .runtime
                .state_mut()
                .activate_output_id(OutputId::from("out-2"));
            assert_eq!(bootstrap.apply_pending_discovery_events().unwrap(), 1);

            bootstrap
                .runtime
                .state_mut()
                .remove_output_id(&OutputId::from("out-2"));

            let applied = bootstrap.apply_pending_discovery_events().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            assert!(
                snapshot
                    .topology
                    .outputs
                    .iter()
                    .all(|output| output.snapshot.id != OutputId::from("out-2"))
            );
            assert_eq!(
                snapshot.topology.active_output_id,
                Some(OutputId::from("out-1"))
            );
            let layer = snapshot
                .topology
                .surfaces
                .iter()
                .find(|surface| surface.id == "wl-output-lost-layer-1")
                .unwrap();
            assert_eq!(layer.output_id, None);
        }

        #[test]
        fn bootstrap_applies_pending_workspace_activate_action_to_controller_and_export() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state
                .workspaces
                .push(spiders_shared::wm::WorkspaceSnapshot {
                    id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["2".into()],
                    focused: false,
                    visible: false,
                    effective_layout: Some(spiders_shared::wm::LayoutRef {
                        name: "master-stack".into(),
                    }),
                });
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-workspace-activate-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-workspace-activate-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            initialize_smithay_workspace_export(
                &bootstrap.controller,
                bootstrap.runtime.state_mut(),
            );
            bootstrap.runtime.state_mut().queue_workspace_action(
                spiders_shared::api::WmAction::ActivateWorkspace {
                    workspace_id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                },
            );

            let applied = bootstrap.apply_pending_workspace_actions().unwrap();
            let snapshot = bootstrap.snapshot();

            assert_eq!(applied, 1);
            assert_eq!(
                bootstrap.controller.state_snapshot().current_workspace_id,
                Some(spiders_shared::ids::WorkspaceId::from("ws-2"))
            );
            assert!(
                snapshot
                    .runtime
                    .state
                    .outputs
                    .known_output_ids
                    .contains(&OutputId::from("out-1"))
                    || snapshot.runtime.state.outputs.active_output_id.is_none()
            );
        }

        #[test]
        fn bootstrap_applies_pending_workspace_assign_action_to_controller_and_export() {
            let authoring_layout_service = test_authoring_layout_service();
            let config = test_config();
            let mut state = test_state_snapshot();
            state.outputs.push(OutputSnapshot {
                id: OutputId::from("out-2"),
                name: "DP-1".into(),
                logical_x: 0,
                logical_y: 0,
                logical_width: 2560,
                logical_height: 1440,
                scale: 1,
                transform: OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            });
            state
                .workspaces
                .push(spiders_shared::wm::WorkspaceSnapshot {
                    id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                    name: "2".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_tags: vec!["2".into()],
                    focused: false,
                    visible: false,
                    effective_layout: Some(spiders_shared::wm::LayoutRef {
                        name: "master-stack".into(),
                    }),
                });
            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-workspace-assign-bootstrap");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-workspace-assign-bootstrap".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            initialize_smithay_workspace_export(
                &bootstrap.controller,
                bootstrap.runtime.state_mut(),
            );
            bootstrap.runtime.state_mut().queue_workspace_action(
                spiders_shared::api::WmAction::AssignWorkspace {
                    workspace_id: spiders_shared::ids::WorkspaceId::from("ws-2"),
                    output_id: OutputId::from("out-2"),
                },
            );

            let applied = bootstrap.apply_pending_workspace_actions().unwrap();

            assert_eq!(applied, 1);
            assert_eq!(
                bootstrap
                    .controller
                    .state_snapshot()
                    .workspace_by_id(&spiders_shared::ids::WorkspaceId::from("ws-2"))
                    .unwrap()
                    .output_id,
                Some(OutputId::from("out-2"))
            );
        }

        #[test]
        fn workspace_export_carries_window_decoration_policy_snapshot() {
            let authoring_layout_service = test_authoring_layout_service();
            let mut config = test_config();
            config.layouts[0].effects_stylesheet =
                "window { appearance: none; } window::titlebar { background: #111; }".into();
            let mut state = test_state_snapshot();
            state.windows.push(spiders_shared::wm::WindowSnapshot {
                id: WindowId::from("smithay-window-1"),
                shell: spiders_shared::wm::ShellKind::XdgToplevel,
                app_id: Some("foot".into()),
                title: Some("terminal".into()),
                class: None,
                instance: None,
                role: None,
                window_type: None,
                mapped: true,
                floating: false,
                floating_rect: None,
                fullscreen: false,
                focused: true,
                urgent: false,
                output_id: Some(OutputId::from("out-1")),
                workspace_id: Some(WorkspaceId::from("ws-1")),
                tags: vec!["1".into()],
            });
            state
                .visible_window_ids
                .push(WindowId::from("smithay-window-1"));

            let controller =
                crate::CompositorController::initialize(authoring_layout_service, config, state)
                    .unwrap();
            let runtime = test_runtime("wayland-test-decoration-policy-export");
            let report = SmithayStartupReport {
                controller: controller.report(),
                output_name: "smithay-test-output".into(),
                seat_name: "smithay-test-seat".into(),
                logical_size: (1280, 720),
                socket_name: Some("wayland-test-decoration-policy-export".into()),
            };
            let mut bootstrap = SmithayBootstrap {
                controller,
                runtime,
                report,
                lifecycle_debug: SmithayLifecycleDebugSnapshot::default(),
            };

            bootstrap.runtime.state_mut().track_test_surface_snapshot(
                crate::backend::BackendSurfaceSnapshot::Window {
                    surface_id: "wl-surface-701".into(),
                    window_id: WindowId::from("smithay-window-1"),
                    output_id: Some(OutputId::from("out-1")),
                },
            );

            initialize_smithay_workspace_export(
                &bootstrap.controller,
                bootstrap.runtime.state_mut(),
            );

            let snapshot = bootstrap.runtime.snapshot();
            let toplevel = snapshot
                .state
                .known_surfaces
                .toplevels
                .iter()
                .find(|surface| surface.surface_id == "wl-surface-701")
                .unwrap();

            assert!(!toplevel.decoration_policy.decorations_visible);
            assert!(!toplevel.decoration_policy.titlebar_visible);
            assert_eq!(
                toplevel
                    .decoration_policy
                    .titlebar_style
                    .background
                    .as_deref(),
                Some("#111")
            );
        }
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{
    SmithayBootstrap, SmithayBootstrapSnapshot, SmithayBootstrapTopologySnapshot,
    SmithayRuntimeError, SmithayRuntimeSnapshot, SmithayStartupReport, SmithayWinitOptions,
    SmithayWinitRuntime, bootstrap_winit, bootstrap_winit_controller,
    bootstrap_winit_controller_with_options, bootstrap_winit_with_options,
    initialize_smithay_workspace_export, initialize_winit_controller,
};

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, thiserror::Error)]
pub enum SmithayRuntimeError {
    #[error("smithay-winit feature is disabled")]
    Disabled,
}

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmithayStartupReport;

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmithayRuntimeSnapshot;

#[cfg(not(feature = "smithay-winit"))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmithayBootstrapSnapshot;
