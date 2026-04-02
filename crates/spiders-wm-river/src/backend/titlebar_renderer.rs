use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use spiders_config::model::TitlebarFontConfig;
use tiny_skia::{
    Color, FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, PremultipliedColorU8, Rect,
    Transform,
};
use tracing::debug;

use crate::backend::plan::TitlebarPlan;

#[derive(Clone)]
struct TitlebarFonts {
    regular: FontArc,
    bold: Option<FontArc>,
}

const REGULAR_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
];

const BOLD_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
];

const SERIF_REGULAR_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
    "/usr/share/fonts/TTF/DejaVuSerif.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSerif-Regular.ttf",
];

const SERIF_BOLD_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSerif-Bold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationSerif-Bold.ttf",
];

const MONO_REGULAR_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationMono-Regular.ttf",
];

const MONO_BOLD_FONT_PATHS: &[&str] = &[
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/truetype/liberation2/LiberationMono-Bold.ttf",
];

static TITLEBAR_FONTS: OnceLock<Option<TitlebarFonts>> = OnceLock::new();
static TITLEBAR_FONT_CACHE: OnceLock<Mutex<std::collections::HashMap<String, Option<FontArc>>>> =
    OnceLock::new();

pub(super) fn render_titlebar_pixels(
    width: i32,
    plan: &TitlebarPlan,
    override_font: Option<&TitlebarFontConfig>,
) -> Option<Vec<u32>> {
    if width <= 0 || plan.height <= 0 {
        return None;
    }

    let mut pixmap = Pixmap::new(width as u32, plan.height as u32)?;
    draw_box_shadow(&mut pixmap, width, plan);

    let mut content_pixmap = Pixmap::new(width as u32, plan.height as u32)?;
    draw_background(&mut content_pixmap, width, plan);
    draw_bottom_border(&mut content_pixmap, width, plan);
    draw_title_text(&mut content_pixmap, width, plan, override_font);

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

fn draw_title_text(
    pixmap: &mut Pixmap,
    width: i32,
    plan: &TitlebarPlan,
    override_font: Option<&TitlebarFontConfig>,
) {
    if plan.title.is_empty() || plan.text_color.alpha == 0 {
        return;
    }

    let available_width = (width - plan.padding_left - plan.padding_right).max(0);
    let available_height =
        (plan.height - plan.padding_top - plan.padding_bottom - plan.border_bottom_width).max(0);
    if available_width <= 0 || available_height <= 0 {
        return;
    }

    let Some(fonts) = titlebar_fonts(plan.font_family.as_deref(), override_font) else {
        return;
    };

    let font = fonts
        .bold
        .as_ref()
        .filter(|_| matches!(plan.font_weight, spiders_scene::FontWeightValue::Bold))
        .unwrap_or(&fonts.regular);

    let scale = PxScale::from(plan.font_size.max(1) as f32);
    let scaled = font.as_scaled(scale);
    let glyph_height = (scaled.ascent() - scaled.descent()).ceil();
    if glyph_height <= 0.0 {
        return;
    }

    let glyphs = layout_glyphs(font, &scaled, width, plan, available_width);
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
) -> Vec<(ab_glyph::GlyphId, f32)> {
    let mut laid_out = Vec::new();
    let mut current_width = 0.0f32;
    let letter_spacing = plan.letter_spacing as f32;

    for character in plan.title.chars() {
        let glyph_id = font.glyph_id(character);
        let advance = scaled.h_advance(glyph_id) + letter_spacing;
        let next_width = if laid_out.is_empty() {
            scaled.h_advance(glyph_id)
        } else {
            current_width + advance
        };
        if next_width > available_width as f32 {
            break;
        }

        let x = if laid_out.is_empty() {
            0.0
        } else {
            current_width + letter_spacing
        };
        laid_out.push((glyph_id, x));
        current_width = if laid_out.len() == 1 {
            scaled.h_advance(glyph_id)
        } else {
            current_width + advance
        };
    }

    if laid_out.is_empty() {
        return laid_out;
    }

    let text_width = laid_out
        .last()
        .map(|(glyph_id, x)| x + scaled.h_advance(*glyph_id))
        .unwrap_or(0.0);

    let start_x = match plan.text_align {
        spiders_scene::TextAlignValue::Right | spiders_scene::TextAlignValue::End => {
            (width - plan.padding_right) as f32 - text_width
        }
        spiders_scene::TextAlignValue::Center => {
            plan.padding_left as f32 + ((available_width as f32 - text_width) / 2.0).max(0.0)
        }
        spiders_scene::TextAlignValue::Left | spiders_scene::TextAlignValue::Start => {
            plan.padding_left.max(0) as f32
        }
    };

    laid_out
        .into_iter()
        .map(|(glyph_id, x)| (glyph_id, start_x + x.max(0.0)))
        .collect()
}

fn titlebar_fonts(
    font_family: Option<&[String]>,
    override_font: Option<&TitlebarFontConfig>,
) -> Option<TitlebarFonts> {
    if let Some(override_font) = override_font {
        let defaults = TITLEBAR_FONTS.get_or_init(default_titlebar_fonts);
        let regular = override_font
            .regular_path
            .as_deref()
            .and_then(load_font_from_path)
            .or_else(|| defaults.as_ref().map(|fonts| fonts.regular.clone()))?;
        let bold = override_font
            .bold_path
            .as_deref()
            .and_then(load_font_from_path)
            .or_else(|| defaults.as_ref().and_then(|fonts| fonts.bold.clone()));
        return Some(TitlebarFonts { regular, bold });
    }

    if let Some(fonts) = font_family.and_then(resolve_fonts_for_family) {
        return Some(fonts);
    }

    TITLEBAR_FONTS
        .get_or_init(default_titlebar_fonts)
        .as_ref()
        .cloned()
}

fn resolve_fonts_for_family(font_family: &[String]) -> Option<TitlebarFonts> {
    for family in font_family {
        let normalized = normalize_font_family_name(family);
        let fonts = match normalized.as_str() {
            "serif" | "dejavu serif" | "liberation serif" => {
                Some((SERIF_REGULAR_FONT_PATHS, SERIF_BOLD_FONT_PATHS))
            }
            "monospace" | "dejavu sans mono" | "liberation mono" => {
                Some((MONO_REGULAR_FONT_PATHS, MONO_BOLD_FONT_PATHS))
            }
            "sans-serif" | "dejavu sans" | "liberation sans" | "system-ui" => {
                Some((REGULAR_FONT_PATHS, BOLD_FONT_PATHS))
            }
            _ => None,
        };

        if let Some((regular_paths, bold_paths)) = fonts {
            let regular = load_font_from_paths(regular_paths)?;
            let bold = load_font_from_paths(bold_paths);
            return Some(TitlebarFonts { regular, bold });
        }
    }

    None
}

fn normalize_font_family_name(family: &str) -> String {
    family
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
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
    shadow: &spiders_scene::BoxShadowValue,
    shadow_color: spiders_scene::ColorValue,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_multiple_outset_shadows() {
        let pixels = render_titlebar_pixels(
            12,
            &TitlebarPlan {
                window_id: spiders_core::WindowId("1".to_string()),
                height: 10,
                offset_x: 0,
                offset_y: 0,
                background: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                border_bottom_width: 0,
                border_bottom_color: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                title: String::new(),
                text_color: spiders_scene::ColorValue {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                },
                text_align: spiders_scene::TextAlignValue::Left,
                font_family: None,
                font_size: 14,
                font_weight: spiders_scene::FontWeightValue::Normal,
                letter_spacing: 0,
                box_shadow: Some(vec![
                    spiders_scene::BoxShadowValue {
                        color: Some(spiders_scene::ColorValue {
                            red: 255,
                            green: 0,
                            blue: 0,
                            alpha: 255,
                        }),
                        offset_x: 0,
                        offset_y: -6,
                        blur_radius: 0,
                        spread_radius: 0,
                        inset: false,
                    },
                    spiders_scene::BoxShadowValue {
                        color: Some(spiders_scene::ColorValue {
                            red: 0,
                            green: 0,
                            blue: 255,
                            alpha: 255,
                        }),
                        offset_x: 0,
                        offset_y: 6,
                        blur_radius: 0,
                        spread_radius: 0,
                        inset: false,
                    },
                ]),
                padding_top: 0,
                padding_right: 0,
                padding_bottom: 0,
                padding_left: 0,
                corner_radius_top_left: 0,
                corner_radius_top_right: 0,
            },
            None,
        )
        .expect("titlebar pixels");

        let top_pixel = pixels[0].to_le_bytes();
        let bottom_pixel = pixels[(9 * 12) as usize].to_le_bytes();

        assert!(top_pixel[2] > 0);
        assert_eq!(top_pixel[0], 0);
        assert!(bottom_pixel[0] > 0);
        assert_eq!(bottom_pixel[2], 0);
    }

    #[test]
    fn masks_rounded_top_corners() {
        let pixels = render_titlebar_pixels(
            10,
            &TitlebarPlan {
                window_id: spiders_core::WindowId("1".to_string()),
                height: 10,
                offset_x: 0,
                offset_y: 0,
                background: spiders_scene::ColorValue {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                },
                border_bottom_width: 0,
                border_bottom_color: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                title: String::new(),
                text_color: spiders_scene::ColorValue {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                },
                text_align: spiders_scene::TextAlignValue::Left,
                font_family: None,
                font_size: 14,
                font_weight: spiders_scene::FontWeightValue::Normal,
                letter_spacing: 0,
                box_shadow: None,
                padding_top: 0,
                padding_right: 0,
                padding_bottom: 0,
                padding_left: 0,
                corner_radius_top_left: 4,
                corner_radius_top_right: 0,
            },
            None,
        )
        .expect("titlebar pixels");

        assert_eq!(pixels[0].to_le_bytes()[3], 0);
        assert!(pixels[4].to_le_bytes()[3] > 0);
        assert!(pixels[(4 * 10) as usize].to_le_bytes()[3] > 0);
    }

    #[test]
    fn rounded_corner_mask_does_not_clip_shadow_layer() {
        let pixels = render_titlebar_pixels(
            10,
            &TitlebarPlan {
                window_id: spiders_core::WindowId("1".to_string()),
                height: 10,
                offset_x: 0,
                offset_y: 0,
                background: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                border_bottom_width: 0,
                border_bottom_color: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                title: String::new(),
                text_color: spiders_scene::ColorValue {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                },
                text_align: spiders_scene::TextAlignValue::Left,
                font_family: None,
                font_size: 14,
                font_weight: spiders_scene::FontWeightValue::Normal,
                letter_spacing: 0,
                box_shadow: Some(vec![spiders_scene::BoxShadowValue {
                    color: Some(spiders_scene::ColorValue {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    }),
                    offset_x: -2,
                    offset_y: -2,
                    blur_radius: 0,
                    spread_radius: 0,
                    inset: false,
                }]),
                padding_top: 0,
                padding_right: 0,
                padding_bottom: 0,
                padding_left: 0,
                corner_radius_top_left: 4,
                corner_radius_top_right: 0,
            },
            None,
        )
        .expect("titlebar pixels");

        let top_left = pixels[0].to_le_bytes();

        assert_eq!(top_left[3], 255);
        assert!(top_left[0] > 0);
    }

    #[test]
    fn rounded_shadow_respects_top_corner_shape() {
        let pixels = render_titlebar_pixels(
            10,
            &TitlebarPlan {
                window_id: spiders_core::WindowId("1".to_string()),
                height: 10,
                offset_x: 0,
                offset_y: 0,
                background: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                border_bottom_width: 0,
                border_bottom_color: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
                title: String::new(),
                text_color: spiders_scene::ColorValue {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                },
                text_align: spiders_scene::TextAlignValue::Left,
                font_family: None,
                font_size: 14,
                font_weight: spiders_scene::FontWeightValue::Normal,
                letter_spacing: 0,
                box_shadow: Some(vec![spiders_scene::BoxShadowValue {
                    color: Some(spiders_scene::ColorValue {
                        red: 0,
                        green: 0,
                        blue: 255,
                        alpha: 255,
                    }),
                    offset_x: 0,
                    offset_y: 0,
                    blur_radius: 0,
                    spread_radius: 0,
                    inset: false,
                }]),
                padding_top: 0,
                padding_right: 0,
                padding_bottom: 0,
                padding_left: 0,
                corner_radius_top_left: 4,
                corner_radius_top_right: 0,
            },
            None,
        )
        .expect("titlebar pixels");

        assert_eq!(pixels[0].to_le_bytes()[3], 0);
        assert!(pixels[4].to_le_bytes()[0] > 0);
    }
}

fn load_font_from_paths(paths: &[&str]) -> Option<FontArc> {
    for path in paths {
        let path = Path::new(path);
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if let Ok(font) = FontArc::try_from_vec(bytes) {
            return Some(font);
        }
    }

    debug!("no usable system font found for titlebar renderer");
    None
}

fn default_titlebar_fonts() -> Option<TitlebarFonts> {
    let regular = load_font_from_paths(REGULAR_FONT_PATHS)?;
    let bold = load_font_from_paths(BOLD_FONT_PATHS);
    Some(TitlebarFonts { regular, bold })
}

fn load_font_from_path(path: &str) -> Option<FontArc> {
    let cache = TITLEBAR_FONT_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));

    if let Ok(guard) = cache.lock() {
        if let Some(font) = guard.get(path) {
            return font.clone();
        }
    }

    let loaded = fs::read(Path::new(path))
        .ok()
        .and_then(|bytes| FontArc::try_from_vec(bytes).ok());

    if let Ok(mut guard) = cache.lock() {
        guard.insert(path.to_string(), loaded.clone());
    }

    loaded
}

fn color_from_scene(color: spiders_scene::ColorValue) -> Color {
    Color::from_rgba8(color.red, color.green, color.blue, color.alpha)
}

fn premultiplied_color(color: Color, coverage: f32) -> PremultipliedColorU8 {
    let mut color = color;
    color.apply_opacity(coverage.clamp(0.0, 1.0));
    color.premultiply().to_color_u8()
}

fn blend_premultiplied_rgba(
    pixels: &mut [PremultipliedColorU8],
    width: i32,
    x: i32,
    y: i32,
    source: PremultipliedColorU8,
) {
    let offset = (y * width + x) as usize;
    let Some(dest) = pixels.get_mut(offset) else {
        return;
    };
    let inv_alpha = 255u16 - u16::from(source.alpha());

    let blended = PremultipliedColorU8::from_rgba(
        (u16::from(source.red()) + ((u16::from(dest.red()) * inv_alpha + 127) / 255)) as u8,
        (u16::from(source.green()) + ((u16::from(dest.green()) * inv_alpha + 127) / 255)) as u8,
        (u16::from(source.blue()) + ((u16::from(dest.blue()) * inv_alpha + 127) / 255)) as u8,
        (u16::from(source.alpha()) + ((u16::from(dest.alpha()) * inv_alpha + 127) / 255)) as u8,
    );
    if let Some(blended) = blended {
        *dest = blended;
    }
}

fn build_top_corner_mask(
    width: i32,
    height: i32,
    corner_radius_top_left: i32,
    corner_radius_top_right: i32,
) -> Option<Mask> {
    if width <= 0 || height <= 0 {
        return None;
    }

    let left_radius = corner_radius_top_left.clamp(0, width).min(height);
    let right_radius = corner_radius_top_right.clamp(0, width).min(height);
    if left_radius == 0 && right_radius == 0 {
        return None;
    }

    let mut mask = Mask::new(width as u32, height as u32)?;
    let rect = Rect::from_xywh(0.0, 0.0, width as f32, height as f32)?;
    fill_top_rounded_shape_mask(&mut mask, rect, left_radius, right_radius);

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
    let shape = TopRoundedShape::new(
        rect.x(),
        rect.y(),
        rect.width(),
        rect.height(),
        corner_radius_top_left,
        corner_radius_top_right,
    );

    shape.fill_pixmap(pixmap, paint, mask);
}

fn fill_top_rounded_shape_mask(
    mask: &mut Mask,
    rect: Rect,
    corner_radius_top_left: i32,
    corner_radius_top_right: i32,
) {
    let shape = TopRoundedShape::new(
        rect.x(),
        rect.y(),
        rect.width(),
        rect.height(),
        corner_radius_top_left,
        corner_radius_top_right,
    );

    shape.fill_mask(mask);
}

struct TopRoundedShape {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    left_radius: f32,
    right_radius: f32,
    max_radius: f32,
}

impl TopRoundedShape {
    fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        corner_radius_top_left: i32,
        corner_radius_top_right: i32,
    ) -> Self {
        let left_radius = corner_radius_top_left.max(0) as f32;
        let right_radius = corner_radius_top_right.max(0) as f32;
        let left_radius = left_radius.min(width).min(height);
        let right_radius = right_radius.min(width).min(height);

        Self {
            x,
            y,
            width,
            height,
            left_radius,
            right_radius,
            max_radius: left_radius.max(right_radius),
        }
    }

    fn fill_pixmap(&self, pixmap: &mut Pixmap, paint: &Paint, mask: Option<&Mask>) {
        self.for_each_part(|part| match part {
            ShapePart::Rect(rect) => pixmap.fill_rect(rect, paint, Transform::identity(), mask),
            ShapePart::Circle {
                center_x,
                center_y,
                radius,
            } => {
                let Some(path) = PathBuilder::from_circle(center_x, center_y, radius) else {
                    return;
                };

                pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), mask);
            }
        });
    }

    fn fill_mask(&self, mask: &mut Mask) {
        self.for_each_part(|part| match part {
            ShapePart::Rect(rect) => fill_mask_rect(mask, rect),
            ShapePart::Circle {
                center_x,
                center_y,
                radius,
            } => fill_mask_circle(mask, center_x, center_y, radius),
        });
    }

    fn for_each_part(&self, mut f: impl FnMut(ShapePart)) {
        let bottom_height = self.height - self.max_radius;
        if let Some(rect) =
            rect_from_xywh(self.x, self.y + self.max_radius, self.width, bottom_height)
        {
            f(ShapePart::Rect(rect));
        }

        if let Some(rect) = rect_from_xywh(
            self.x + self.left_radius,
            self.y,
            self.width - self.left_radius - self.right_radius,
            self.max_radius,
        ) {
            f(ShapePart::Rect(rect));
        }

        if self.left_radius > 0.0 {
            f(ShapePart::Circle {
                center_x: self.x + self.left_radius,
                center_y: self.y + self.left_radius,
                radius: self.left_radius,
            });

            if let Some(rect) = rect_from_xywh(
                self.x,
                self.y + self.left_radius,
                self.left_radius,
                self.max_radius - self.left_radius,
            ) {
                f(ShapePart::Rect(rect));
            }
        }

        if self.right_radius > 0.0 {
            f(ShapePart::Circle {
                center_x: self.x + self.width - self.right_radius,
                center_y: self.y + self.right_radius,
                radius: self.right_radius,
            });

            if let Some(rect) = rect_from_xywh(
                self.x + self.width - self.right_radius,
                self.y + self.right_radius,
                self.right_radius,
                self.max_radius - self.right_radius,
            ) {
                f(ShapePart::Rect(rect));
            }
        }
    }
}

enum ShapePart {
    Rect(Rect),
    Circle {
        center_x: f32,
        center_y: f32,
        radius: f32,
    },
}

fn rect_from_xywh(x: f32, y: f32, width: f32, height: f32) -> Option<Rect> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Rect::from_xywh(x, y, width, height)
}

fn fill_mask_rect(mask: &mut Mask, rect: Rect) {
    let path = PathBuilder::from_rect(rect);
    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
}

fn fill_mask_circle(mask: &mut Mask, center_x: f32, center_y: f32, radius: f32) {
    if radius <= 0.0 {
        return;
    }

    let Some(path) = PathBuilder::from_circle(center_x, center_y, radius) else {
        return;
    };

    mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
}
