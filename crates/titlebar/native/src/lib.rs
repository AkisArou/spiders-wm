use std::fs;
use std::path::Path;

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use spiders_config::model::TitlebarFontConfig;
use spiders_fonts_native::{NativeFontResolver, ResolvedNativeFont};
use spiders_titlebar_core::{TitlebarPlan, titlebar_button_colors, titlebar_text_left_inset};
use tiny_skia::{
    Color, FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, PremultipliedColorU8, Rect,
    Transform,
};

#[derive(Clone)]
struct TitlebarFonts {
    regular: FontArc,
    bold: Option<FontArc>,
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
