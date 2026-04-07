use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use spiders_config::model::TitlebarFontConfig;
use spiders_core::LayoutRect;
use spiders_css::{
    FontFamilyName, FontQuery, FontWeightValue, LengthPercentage, TextAlignValue,
    TextTransformValue,
};
use spiders_fonts_native::{
    CachedNativeFontResolver, FontDbNativeFontResolver, NativeFontResolver, ResolvedNativeFont,
};
use spiders_scene::{ComputedStyle, LayoutSnapshotNode};
use spiders_titlebar_core::{
    TitlebarPlan, titlebar_button_colors, titlebar_icon_nodes_from_data, titlebar_icon_paths,
    titlebar_icon_view_box, titlebar_text_left_inset,
};
use tiny_skia::{
    Color, FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, PremultipliedColorU8, Rect,
    Transform,
};

#[derive(Clone)]
struct TitlebarFonts {
    regular: FontArc,
    bold: Option<FontArc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterizedTitlebar {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}

pub fn render_titlebar_pixels(
    width: i32,
    plan: &TitlebarPlan,
    override_font: Option<&TitlebarFontConfig>,
    resolver: &dyn NativeFontResolver,
) -> Option<Vec<u32>> {
    if width <= 0 || plan.height <= 0 {
        return None;
    }

    let mut pixmap = Pixmap::new(width as u32, plan.height as u32)?;
    draw_box_shadow(&mut pixmap, width, plan);

    let mut content_pixmap = Pixmap::new(width as u32, plan.height as u32)?;
    draw_background(&mut content_pixmap, width, plan);
    draw_bottom_border(&mut content_pixmap, width, plan);
    draw_titlebar_buttons(&mut content_pixmap, plan);
    draw_title_text(&mut content_pixmap, width, plan, override_font, resolver);

    if let Some(mask) = build_top_corner_mask(
        width,
        plan.height,
        plan.corner_radius_top_left,
        plan.corner_radius_top_right,
    ) {
        content_pixmap.apply_mask(&mask);
    }

    pixmap.draw_pixmap(
        0,
        0,
        content_pixmap.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );

    Some(
        pixmap
            .data()
            .chunks_exact(4)
            .map(|rgba| u32::from_le_bytes([rgba[2], rgba[1], rgba[0], rgba[3]]))
            .collect(),
    )
}

pub fn render_titlebar_snapshot(
    root: &LayoutSnapshotNode,
    scale: f32,
    override_font: Option<&TitlebarFontConfig>,
) -> Option<RasterizedTitlebar> {
    let rect = root.rect();
    let width = (rect.width * scale).round().max(1.0) as i32;
    let height = (rect.height * scale).round().max(1.0) as i32;
    if width <= 0 || height <= 0 {
        return None;
    }

    let mut pixmap = Pixmap::new(width as u32, height as u32)?;
    let resolver = shared_native_font_resolver();
    draw_snapshot_node(&mut pixmap, root, rect.x, rect.y, scale.max(0.1), override_font, resolver);

    Some(RasterizedTitlebar { width, height, pixels: pixmap_argb_bytes(&pixmap) })
}

fn shared_native_font_resolver() -> &'static CachedNativeFontResolver<FontDbNativeFontResolver> {
    static RESOLVER: OnceLock<CachedNativeFontResolver<FontDbNativeFontResolver>> = OnceLock::new();
    RESOLVER.get_or_init(|| CachedNativeFontResolver::new(FontDbNativeFontResolver::default()))
}

fn draw_background(pixmap: &mut Pixmap, width: i32, plan: &TitlebarPlan) {
    let Some(rect) = Rect::from_xywh(0.0, 0.0, width as f32, plan.height as f32) else {
        return;
    };

    let mut paint = Paint::default();
    paint.set_color(color_from_scene(plan.background));
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
}

fn draw_bottom_border(pixmap: &mut Pixmap, width: i32, plan: &TitlebarPlan) {
    let border_height = plan.border_bottom_width.clamp(0, plan.height);
    if border_height <= 0 || plan.border_bottom_color.alpha == 0 {
        return;
    }

    let Some(rect) = Rect::from_xywh(
        0.0,
        (plan.height - border_height) as f32,
        width as f32,
        border_height as f32,
    ) else {
        return;
    };

    let mut paint = Paint::default();
    paint.set_color(color_from_scene(plan.border_bottom_color));
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
}

fn draw_titlebar_buttons(pixmap: &mut Pixmap, plan: &TitlebarPlan) {
    for button in &plan.buttons {
        let Some(rect) = Rect::from_xywh(
            button.rect.x as f32,
            button.rect.y as f32,
            button.rect.width.max(0) as f32,
            button.rect.height.max(0) as f32,
        ) else {
            continue;
        };

        let mut paint = Paint::default();
        let color = titlebar_button_colors(button.kind);
        paint.set_color(Color::from_rgba8(color.red, color.green, color.blue, color.alpha));
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

fn draw_title_text(
    pixmap: &mut Pixmap,
    width: i32,
    plan: &TitlebarPlan,
    override_font: Option<&TitlebarFontConfig>,
    resolver: &dyn NativeFontResolver,
) {
    if plan.title.is_empty() || plan.text_color.alpha == 0 {
        return;
    }

    let effective_left_padding = titlebar_text_left_inset(plan);
    let available_width = (width - effective_left_padding - plan.padding_right).max(0);
    let available_height =
        (plan.height - plan.padding_top - plan.padding_bottom - plan.border_bottom_width).max(0);
    if available_width <= 0 || available_height <= 0 {
        return;
    }

    let Some(fonts) = titlebar_fonts(plan, override_font, resolver) else {
        return;
    };

    let font = fonts
        .bold
        .as_ref()
        .filter(|_| matches!(plan.font.weight, spiders_css::FontWeightValue::Bold))
        .unwrap_or(&fonts.regular);

    let scale = PxScale::from(plan.font.size_px.max(1) as f32);
    let scaled = font.as_scaled(scale);
    let glyph_height = (scaled.ascent() - scaled.descent()).ceil();
    if glyph_height <= 0.0 {
        return;
    }

    let glyphs = layout_glyphs(font, &scaled, width, plan, available_width, effective_left_padding);
    if glyphs.is_empty() {
        return;
    }

    let baseline_y = plan.padding_top.max(0) as f32
        + ((available_height as f32 - glyph_height) / 2.0).max(0.0)
        + scaled.ascent();

    let base_text_color = color_from_scene(plan.text_color);
    let pixels = pixmap.pixels_mut();
    for (glyph_id, x) in glyphs {
        let glyph = glyph_id.with_scale_and_position(scale, point(x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            outlined.draw(|glyph_x, glyph_y, coverage| {
                let pixel_x = glyph_x as i32;
                let pixel_y = glyph_y as i32;
                if pixel_x < 0
                    || pixel_y < 0
                    || pixel_x >= width
                    || pixel_y >= plan.height - plan.border_bottom_width.clamp(0, plan.height)
                {
                    return;
                }

                blend_premultiplied_rgba(
                    pixels,
                    width,
                    pixel_x,
                    pixel_y,
                    premultiplied_color(base_text_color, coverage),
                );
            });
        }
    }
}

fn layout_glyphs(
    font: &FontArc,
    scaled: &ab_glyph::PxScaleFont<&FontArc>,
    width: i32,
    plan: &TitlebarPlan,
    available_width: i32,
    effective_left_padding: i32,
) -> Vec<(ab_glyph::GlyphId, f32)> {
    let mut laid_out = Vec::new();
    let mut current_width = 0.0f32;
    let letter_spacing = plan.letter_spacing as f32;

    for character in plan.title.chars() {
        let glyph_id = font.glyph_id(character);
        let advance = scaled.h_advance(glyph_id) + letter_spacing;
        let next_width =
            if laid_out.is_empty() { scaled.h_advance(glyph_id) } else { current_width + advance };
        if next_width > available_width as f32 {
            break;
        }

        let x = if laid_out.is_empty() { 0.0 } else { current_width + letter_spacing };
        laid_out.push((glyph_id, x));
        current_width =
            if laid_out.len() == 1 { scaled.h_advance(glyph_id) } else { current_width + advance };
    }

    if laid_out.is_empty() {
        return laid_out;
    }

    let text_width =
        laid_out.last().map(|(glyph_id, x)| x + scaled.h_advance(*glyph_id)).unwrap_or(0.0);

    let start_x = match plan.text_align {
        spiders_css::TextAlignValue::Right | spiders_css::TextAlignValue::End => {
            (width - plan.padding_right) as f32 - text_width
        }
        spiders_css::TextAlignValue::Center => {
            effective_left_padding as f32 + ((available_width as f32 - text_width) / 2.0).max(0.0)
        }
        spiders_css::TextAlignValue::Left | spiders_css::TextAlignValue::Start => {
            effective_left_padding.max(0) as f32
        }
    };

    laid_out.into_iter().map(|(glyph_id, x)| (glyph_id, start_x + x.max(0.0))).collect()
}

fn titlebar_fonts(
    plan: &TitlebarPlan,
    override_font: Option<&TitlebarFontConfig>,
    resolver: &dyn NativeFontResolver,
) -> Option<TitlebarFonts> {
    if let Some(override_font) = override_font {
        let regular = override_font.regular_path.as_deref().and_then(load_font_from_path)?;
        let bold = override_font.bold_path.as_deref().and_then(load_font_from_path);
        return Some(TitlebarFonts { regular, bold });
    }

    let regular = resolve_font_arc(resolver.resolve(&plan.font)?)?;
    let bold = if matches!(plan.font.weight, spiders_css::FontWeightValue::Bold) {
        Some(regular.clone())
    } else {
        None
    };

    Some(TitlebarFonts { regular, bold })
}

fn titlebar_fonts_for_query(
    font: &FontQuery,
    override_font: Option<&TitlebarFontConfig>,
    resolver: &dyn NativeFontResolver,
) -> Option<TitlebarFonts> {
    if let Some(override_font) = override_font {
        let regular = override_font.regular_path.as_deref().and_then(load_font_from_path)?;
        let bold = override_font.bold_path.as_deref().and_then(load_font_from_path);
        return Some(TitlebarFonts { regular, bold });
    }

    let regular = resolve_font_arc(resolver.resolve(font)?)?;
    let bold =
        if matches!(font.weight, FontWeightValue::Bold) { Some(regular.clone()) } else { None };

    Some(TitlebarFonts { regular, bold })
}

fn resolve_font_arc(font: ResolvedNativeFont) -> Option<FontArc> {
    FontArc::try_from_vec(font.data.as_ref().clone()).ok()
}

fn load_font_from_path(path: &str) -> Option<FontArc> {
    let bytes = fs::read(Path::new(path)).ok()?;
    FontArc::try_from_vec(bytes).ok()
}

fn draw_box_shadow(pixmap: &mut Pixmap, width: i32, plan: &TitlebarPlan) {
    let Some(shadows) = plan.box_shadow.as_ref() else {
        return;
    };

    for shadow in shadows.iter().filter(|shadow| !shadow.inset) {
        draw_outset_box_shadow(
            pixmap,
            width,
            plan.height,
            plan.corner_radius_top_left,
            plan.corner_radius_top_right,
            shadow,
            shadow.color.unwrap_or(plan.text_color),
        );
    }
}

fn draw_outset_box_shadow(
    pixmap: &mut Pixmap,
    width: i32,
    height: i32,
    corner_radius_top_left: i32,
    corner_radius_top_right: i32,
    shadow: &spiders_css::BoxShadowValue,
    shadow_color: spiders_css::ColorValue,
) {
    let blur_radius = shadow.blur_radius.max(0);
    let blur_steps = blur_radius.clamp(0, 16);
    let base_alpha_scale = if blur_steps == 0 { 1.0 } else { 0.22 };

    let outer_left = shadow.offset_x - shadow.spread_radius - blur_radius;
    let outer_top = shadow.offset_y - shadow.spread_radius - blur_radius;
    let outer_width = width + (shadow.spread_radius + blur_radius) * 2;
    let outer_height = height + (shadow.spread_radius + blur_radius) * 2;
    if outer_width <= 0 || outer_height <= 0 {
        return;
    }

    let layer_count = blur_steps.max(1);
    for step in (0..layer_count).rev() {
        let inset = step;
        let layer_width = outer_width - inset * 2;
        let layer_height = outer_height - inset * 2;
        if layer_width <= 0 || layer_height <= 0 {
            continue;
        }

        let alpha_scale = if blur_steps == 0 {
            1.0
        } else {
            ((layer_count - step) as f32 / layer_count as f32) * base_alpha_scale
        };
        let alpha = (f32::from(shadow_color.alpha) * alpha_scale).round() as u8;
        if alpha == 0 {
            continue;
        }

        let Some(rect) = Rect::from_xywh(
            (outer_left + inset) as f32,
            (outer_top + inset) as f32,
            layer_width as f32,
            layer_height as f32,
        ) else {
            continue;
        };

        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(
            shadow_color.red,
            shadow_color.green,
            shadow_color.blue,
            alpha,
        ));

        let expansion = shadow.spread_radius + blur_radius - inset;
        fill_top_rounded_shape(
            pixmap,
            rect,
            (corner_radius_top_left + expansion).max(0),
            (corner_radius_top_right + expansion).max(0),
            &paint,
            None,
        );
    }
}

fn build_top_corner_mask(
    width: i32,
    height: i32,
    corner_radius_top_left: i32,
    corner_radius_top_right: i32,
) -> Option<Mask> {
    let mut mask = Mask::new(width as u32, height as u32)?;
    let mut pixmap = Pixmap::new(width as u32, height as u32)?;
    let rect = Rect::from_xywh(0.0, 0.0, width as f32, height as f32)?;
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(255, 255, 255, 255));
    fill_top_rounded_shape(
        &mut pixmap,
        rect,
        corner_radius_top_left,
        corner_radius_top_right,
        &paint,
        None,
    );
    mask.fill_path(&PathBuilder::from_rect(rect), FillRule::Winding, true, Transform::identity());
    Some(mask)
}

fn fill_top_rounded_shape(
    pixmap: &mut Pixmap,
    rect: Rect,
    corner_radius_top_left: i32,
    corner_radius_top_right: i32,
    paint: &Paint,
    mask: Option<&Mask>,
) {
    let mut builder = PathBuilder::new();
    let left = rect.left();
    let top = rect.top();
    let right = rect.right();
    let bottom = rect.bottom();
    let radius_left = corner_radius_top_left.max(0) as f32;
    let radius_right = corner_radius_top_right.max(0) as f32;

    builder.move_to(left, bottom);
    builder.line_to(left, top + radius_left);
    if radius_left > 0.0 {
        builder.quad_to(left, top, left + radius_left, top);
    } else {
        builder.line_to(left, top);
    }
    builder.line_to(right - radius_right, top);
    if radius_right > 0.0 {
        builder.quad_to(right, top, right, top + radius_right);
    } else {
        builder.line_to(right, top);
    }
    builder.line_to(right, bottom);
    builder.close();

    if let Some(path) = builder.finish() {
        pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), mask);
    }
}

fn color_from_scene(color: spiders_css::ColorValue) -> Color {
    Color::from_rgba8(color.red, color.green, color.blue, color.alpha)
}

fn draw_snapshot_node(
    pixmap: &mut Pixmap,
    node: &LayoutSnapshotNode,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
    override_font: Option<&TitlebarFontConfig>,
    resolver: &dyn NativeFontResolver,
) {
    let Some(style) = snapshot_node_style(node) else {
        for child in node.children() {
            draw_snapshot_node(pixmap, child, origin_x, origin_y, scale, override_font, resolver);
        }
        return;
    };

    if matches!(style.display, Some(spiders_css::Display::None)) {
        return;
    }

    draw_snapshot_background_and_border(pixmap, node, style, origin_x, origin_y, scale);
    draw_snapshot_icon(pixmap, node, style, origin_x, origin_y, scale);
    draw_snapshot_text(pixmap, node, style, origin_x, origin_y, scale, override_font, resolver);

    for child in node.children() {
        draw_snapshot_node(pixmap, child, origin_x, origin_y, scale, override_font, resolver);
    }
}

fn snapshot_node_style(node: &LayoutSnapshotNode) -> Option<&ComputedStyle> {
    node.styles().map(|styles| &styles.layout)
}

fn draw_snapshot_background_and_border(
    pixmap: &mut Pixmap,
    node: &LayoutSnapshotNode,
    style: &ComputedStyle,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
) {
    let rect = scaled_local_rect(node.rect(), origin_x, origin_y, scale);
    if rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }

    if let Some(background) = style.background {
        let mut paint = Paint::default();
        paint.set_color(color_from_scene(apply_style_opacity(background, style.opacity)));
        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
    }

    let Some(border) = style.border else {
        return;
    };
    if matches!(
        style.border_style.map(|edges| edges.top),
        Some(spiders_css::BorderStyleValue::None)
    ) {
        return;
    }

    let border_color = style
        .border_color
        .or_else(|| style.border_side_colors.and_then(|colors| colors.top))
        .unwrap_or(spiders_css::ColorValue { red: 0, green: 0, blue: 0, alpha: 0 });
    if border_color.alpha == 0 {
        return;
    }

    let mut paint = Paint::default();
    paint.set_color(color_from_scene(apply_style_opacity(border_color, style.opacity)));

    let top = (length_to_px(border.top) as f32 * scale).round().max(0.0);
    let right = (length_to_px(border.right) as f32 * scale).round().max(0.0);
    let bottom = (length_to_px(border.bottom) as f32 * scale).round().max(0.0);
    let left = (length_to_px(border.left) as f32 * scale).round().max(0.0);

    fill_border_rect(pixmap, &paint, rect.x(), rect.y(), rect.width(), top);
    fill_border_rect(pixmap, &paint, rect.right() - right, rect.y(), right, rect.height());
    fill_border_rect(pixmap, &paint, rect.x(), rect.bottom() - bottom, rect.width(), bottom);
    fill_border_rect(pixmap, &paint, rect.x(), rect.y(), left, rect.height());
}

fn fill_border_rect(pixmap: &mut Pixmap, paint: &Paint, x: f32, y: f32, w: f32, h: f32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    if let Some(rect) = Rect::from_xywh(x, y, w, h) {
        pixmap.fill_rect(rect, paint, Transform::identity(), None);
    }
}

fn draw_snapshot_text(
    pixmap: &mut Pixmap,
    node: &LayoutSnapshotNode,
    style: &ComputedStyle,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
    override_font: Option<&TitlebarFontConfig>,
    resolver: &dyn NativeFontResolver,
) {
    let LayoutSnapshotNode::Content { text: Some(text), .. } = node else {
        return;
    };
    if text.is_empty() {
        return;
    }

    let color = style.color.unwrap_or(spiders_css::ColorValue {
        red: 255,
        green: 255,
        blue: 255,
        alpha: 255,
    });
    if color.alpha == 0 {
        return;
    }

    let font_query = snapshot_font_query(style, scale);
    let Some(fonts) = titlebar_fonts_for_query(&font_query, override_font, resolver) else {
        return;
    };
    let font = fonts
        .bold
        .as_ref()
        .filter(|_| matches!(font_query.weight, FontWeightValue::Bold))
        .unwrap_or(&fonts.regular);

    let rect = scaled_local_rect(node.rect(), origin_x, origin_y, scale);
    let padding = snapshot_padding(style, scale);
    let available_width = (rect.width() - padding.1 - padding.3).round() as i32;
    let available_height = (rect.height() - padding.0 - padding.2).round() as i32;
    if available_width <= 0 || available_height <= 0 {
        return;
    }

    let transformed =
        apply_text_transform(style.text_transform.unwrap_or(TextTransformValue::None), text);
    let scale_px = PxScale::from(font_query.size_px.max(1) as f32);
    let scaled = font.as_scaled(scale_px);
    let glyph_height = (scaled.ascent() - scaled.descent()).ceil();
    if glyph_height <= 0.0 {
        return;
    }

    let glyphs = layout_text_glyphs(
        font,
        &scaled,
        &transformed,
        available_width,
        style.letter_spacing.unwrap_or(0.0),
    );
    if glyphs.is_empty() {
        return;
    }

    let text_width =
        glyphs.last().map(|(glyph_id, x)| x + scaled.h_advance(*glyph_id)).unwrap_or(0.0);
    let start_x = match style.text_align.unwrap_or(TextAlignValue::Left) {
        TextAlignValue::Right | TextAlignValue::End => {
            rect.right() - padding.1 - text_width.max(0.0)
        }
        TextAlignValue::Center => {
            rect.x() + padding.3 + ((available_width as f32 - text_width) / 2.0).max(0.0)
        }
        TextAlignValue::Left | TextAlignValue::Start => rect.x() + padding.3,
    };
    let baseline_y = rect.y()
        + padding.0
        + ((available_height as f32 - glyph_height) / 2.0).max(0.0)
        + scaled.ascent();

    let width = pixmap.width() as i32;
    let height = pixmap.height() as i32;
    let base_text_color = color_from_scene(apply_style_opacity(color, style.opacity));
    let pixels = pixmap.pixels_mut();
    for (glyph_id, x) in glyphs {
        let glyph = glyph_id.with_scale_and_position(scale_px, point(start_x + x, baseline_y));
        if let Some(outlined) = font.outline_glyph(glyph) {
            outlined.draw(|glyph_x, glyph_y, coverage| {
                let pixel_x = glyph_x as i32;
                let pixel_y = glyph_y as i32;
                if pixel_x < 0 || pixel_y < 0 || pixel_x >= width || pixel_y >= height {
                    return;
                }

                blend_premultiplied_rgba(
                    pixels,
                    width,
                    pixel_x,
                    pixel_y,
                    premultiplied_color(base_text_color, coverage),
                );
            });
        }
    }
}

fn draw_snapshot_icon(
    pixmap: &mut Pixmap,
    node: &LayoutSnapshotNode,
    style: &ComputedStyle,
    origin_x: f32,
    origin_y: f32,
    scale: f32,
) {
    let LayoutSnapshotNode::Content { meta, .. } = node else {
        return;
    };
    let Some(icon_nodes) = titlebar_icon_nodes_from_data(&meta.data) else {
        return;
    };
    let path_data = titlebar_icon_paths(&icon_nodes);
    if path_data.is_empty() {
        return;
    }

    let rect = scaled_local_rect(node.rect(), origin_x, origin_y, scale);
    let padding = snapshot_padding(style, scale);
    let left = rect.x() + padding.3;
    let top = rect.y() + padding.0;
    let width = (rect.width() - padding.1 - padding.3).max(0.0);
    let height = (rect.height() - padding.0 - padding.2).max(0.0);
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let color = apply_style_opacity(
        style.color.unwrap_or(spiders_css::ColorValue {
            red: 255,
            green: 255,
            blue: 255,
            alpha: 255,
        }),
        style.opacity,
    );
    if color.alpha == 0 {
        return;
    }

    let view_box = titlebar_icon_view_box(&icon_nodes)
        .as_deref()
        .and_then(parse_view_box)
        .unwrap_or((0.0, 0.0, 16.0, 16.0));
    let Some(transform) = icon_view_box_transform(left, top, width, height, view_box) else {
        return;
    };

    let mut paint = Paint::default();
    paint.set_color(color_from_scene(color));
    paint.anti_alias = true;

    for d in path_data {
        if let Some(path) = parse_svg_path(&d) {
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
    }
}

fn scaled_local_rect(rect: LayoutRect, origin_x: f32, origin_y: f32, scale: f32) -> Rect {
    Rect::from_xywh(
        (rect.x - origin_x) * scale,
        (rect.y - origin_y) * scale,
        (rect.width * scale).max(0.0),
        (rect.height * scale).max(0.0),
    )
    .unwrap_or_else(|| Rect::from_xywh(0.0, 0.0, 0.0, 0.0).expect("zero rect"))
}

fn snapshot_padding(style: &ComputedStyle, scale: f32) -> (f32, f32, f32, f32) {
    let Some(padding) = style.padding else {
        return (0.0, 0.0, 0.0, 0.0);
    };

    (
        length_to_px(padding.top) as f32 * scale,
        length_to_px(padding.right) as f32 * scale,
        length_to_px(padding.bottom) as f32 * scale,
        length_to_px(padding.left) as f32 * scale,
    )
}

fn parse_view_box(view_box: &str) -> Option<(f32, f32, f32, f32)> {
    let values = view_box
        .split(|character: char| character.is_ascii_whitespace() || character == ',')
        .filter(|part| !part.is_empty())
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 4 || values[2] <= 0.0 || values[3] <= 0.0 {
        return None;
    }
    Some((values[0], values[1], values[2], values[3]))
}

fn icon_view_box_transform(
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    view_box: (f32, f32, f32, f32),
) -> Option<Transform> {
    let (view_box_x, view_box_y, view_box_width, view_box_height) = view_box;
    if width <= 0.0 || height <= 0.0 || view_box_width <= 0.0 || view_box_height <= 0.0 {
        return None;
    }

    let fit_scale = (width / view_box_width).min(height / view_box_height);
    if !fit_scale.is_finite() || fit_scale <= 0.0 {
        return None;
    }

    let translate_x = left + (width - view_box_width * fit_scale) / 2.0 - view_box_x * fit_scale;
    let translate_y = top + (height - view_box_height * fit_scale) / 2.0 - view_box_y * fit_scale;
    Some(Transform::from_scale(fit_scale, fit_scale).post_translate(translate_x, translate_y))
}

fn parse_svg_path(data: &str) -> Option<tiny_skia::Path> {
    let segments = svgtypes::SimplifyingPathParser::from(data);
    let mut builder = PathBuilder::new();

    for segment in segments {
        let Ok(segment) = segment else {
            return None;
        };
        match segment {
            svgtypes::SimplePathSegment::MoveTo { x, y } => builder.move_to(x as f32, y as f32),
            svgtypes::SimplePathSegment::LineTo { x, y } => builder.line_to(x as f32, y as f32),
            svgtypes::SimplePathSegment::Quadratic { x1, y1, x, y } => {
                builder.quad_to(x1 as f32, y1 as f32, x as f32, y as f32)
            }
            svgtypes::SimplePathSegment::CurveTo { x1, y1, x2, y2, x, y } => {
                builder.cubic_to(x1 as f32, y1 as f32, x2 as f32, y2 as f32, x as f32, y as f32)
            }
            svgtypes::SimplePathSegment::ClosePath => builder.close(),
        }
    }

    builder.finish()
}

fn snapshot_font_query(style: &ComputedStyle, scale: f32) -> FontQuery {
    FontQuery {
        families: style
            .font_family
            .clone()
            .unwrap_or_else(|| vec![FontFamilyName::SystemUi, FontFamilyName::SansSerif]),
        weight: style.font_weight.unwrap_or(FontWeightValue::Normal),
        size_px: match style.font_size {
            Some(LengthPercentage::Px(value)) | Some(LengthPercentage::Percent(value)) => {
                (value * scale).round() as i32
            }
            None => (14.0 * scale).round() as i32,
        }
        .clamp(8, 72),
    }
}

fn layout_text_glyphs(
    font: &FontArc,
    scaled: &ab_glyph::PxScaleFont<&FontArc>,
    text: &str,
    available_width: i32,
    letter_spacing: f32,
) -> Vec<(ab_glyph::GlyphId, f32)> {
    let mut laid_out = Vec::new();
    let mut current_width = 0.0f32;

    for character in text.chars() {
        let glyph_id = font.glyph_id(character);
        let advance = scaled.h_advance(glyph_id) + letter_spacing;
        let next_width =
            if laid_out.is_empty() { scaled.h_advance(glyph_id) } else { current_width + advance };
        if next_width > available_width as f32 {
            break;
        }

        let x = if laid_out.is_empty() { 0.0 } else { current_width + letter_spacing };
        laid_out.push((glyph_id, x));
        current_width =
            if laid_out.len() == 1 { scaled.h_advance(glyph_id) } else { current_width + advance };
    }

    laid_out
}

fn apply_text_transform(transform: TextTransformValue, text: &str) -> String {
    match transform {
        TextTransformValue::None => text.to_string(),
        TextTransformValue::Uppercase => text.to_uppercase(),
        TextTransformValue::Lowercase => text.to_lowercase(),
        TextTransformValue::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut at_word_start = true;
            for character in text.chars() {
                if at_word_start && character.is_alphanumeric() {
                    result.extend(character.to_uppercase());
                    at_word_start = false;
                } else {
                    result.push(character);
                    if !character.is_alphanumeric() {
                        at_word_start = true;
                    }
                }
            }
            result
        }
    }
}

fn apply_style_opacity(
    color: spiders_css::ColorValue,
    opacity: Option<f32>,
) -> spiders_css::ColorValue {
    let alpha = (f32::from(color.alpha) * opacity.unwrap_or(1.0).clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8;
    spiders_css::ColorValue { alpha, ..color }
}

fn length_to_px(length: LengthPercentage) -> i32 {
    match length {
        LengthPercentage::Px(value) | LengthPercentage::Percent(value) => value.round() as i32,
    }
    .max(0)
}

fn pixmap_argb_bytes(pixmap: &Pixmap) -> Vec<u8> {
    pixmap.data().chunks_exact(4).flat_map(|rgba| [rgba[2], rgba[1], rgba[0], rgba[3]]).collect()
}

fn premultiplied_color(color: Color, coverage: f32) -> PremultipliedColorU8 {
    let alpha = (color.alpha() * coverage * 255.0).round().clamp(0.0, 255.0) as u8;
    let red = (color.red() * color.alpha() * coverage * 255.0).round().clamp(0.0, 255.0) as u8;
    let green = (color.green() * color.alpha() * coverage * 255.0).round().clamp(0.0, 255.0) as u8;
    let blue = (color.blue() * color.alpha() * coverage * 255.0).round().clamp(0.0, 255.0) as u8;
    PremultipliedColorU8::from_rgba(red, green, blue, alpha)
        .unwrap_or(PremultipliedColorU8::TRANSPARENT)
}

fn blend_premultiplied_rgba(
    pixels: &mut [PremultipliedColorU8],
    width: i32,
    x: i32,
    y: i32,
    source: PremultipliedColorU8,
) {
    let index = (y * width + x) as usize;
    let Some(destination) = pixels.get(index).copied() else {
        return;
    };

    let src_a = u32::from(source.alpha());
    let dst_a = u32::from(destination.alpha());
    let inv_src_a = 255 - src_a;

    let blend = |src: u8, dst: u8| -> u8 {
        let value = u32::from(src) + ((u32::from(dst) * inv_src_a + 127) / 255);
        value.min(255) as u8
    };

    let alpha = (src_a + ((dst_a * inv_src_a + 127) / 255)).min(255) as u8;
    pixels[index] = PremultipliedColorU8::from_rgba(
        blend(source.red(), destination.red()),
        blend(source.green(), destination.green()),
        blend(source.blue(), destination.blue()),
        alpha,
    )
    .unwrap_or(PremultipliedColorU8::TRANSPARENT);
}

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_core::{LayoutNodeMeta, LayoutRect};
    use spiders_scene::{LayoutSnapshotNode, SceneNodeStyle};
    use spiders_titlebar_core::{TITLEBAR_ICON_CHILDREN_KEY, TitlebarIconNode};

    #[test]
    fn render_titlebar_snapshot_draws_icon_paths_from_metadata() {
        let mut meta = LayoutNodeMeta::default();
        meta.name = Some("titlebar-icon".into());
        meta.data.insert(
            TITLEBAR_ICON_CHILDREN_KEY.to_string(),
            serde_json::to_string(&vec![TitlebarIconNode::Svg {
                view_box: Some("0 0 16 16".into()),
                children: vec![TitlebarIconNode::Path { d: "M2 2 L14 2 L14 14 L2 14 Z".into() }],
            }])
            .expect("icon metadata should serialize"),
        );

        let root = LayoutSnapshotNode::Content {
            meta: LayoutNodeMeta::default(),
            rect: LayoutRect { x: 0.0, y: 0.0, width: 32.0, height: 32.0 },
            styles: Some(SceneNodeStyle { layout: ComputedStyle::default(), titlebar: None }),
            text: None,
            children: vec![LayoutSnapshotNode::Content {
                meta,
                rect: LayoutRect { x: 4.0, y: 4.0, width: 24.0, height: 24.0 },
                styles: Some(SceneNodeStyle {
                    layout: ComputedStyle {
                        color: Some(spiders_css::ColorValue {
                            red: 255,
                            green: 255,
                            blue: 255,
                            alpha: 255,
                        }),
                        ..ComputedStyle::default()
                    },
                    titlebar: None,
                }),
                text: None,
                children: Vec::new(),
            }],
        };

        let rasterized =
            render_titlebar_snapshot(&root, 1.0, None).expect("snapshot should render");

        assert_eq!(rasterized.width, 32);
        assert_eq!(rasterized.height, 32);
        assert!(rasterized.pixels.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn parse_svg_path_rejects_invalid_path_data() {
        assert!(parse_svg_path("M 0 0 L").is_none());
    }
}
