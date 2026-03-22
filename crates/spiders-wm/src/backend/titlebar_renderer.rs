use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ab_glyph::{point, Font, FontArc, PxScale, ScaleFont};
use spiders_config::model::TitlebarFontConfig;
use tiny_skia::{Color, Paint, Pixmap, Rect, Transform};
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
    draw_background(&mut pixmap, width, plan);
    draw_bottom_border(&mut pixmap, width, plan);
    draw_title_text(&mut pixmap, width, plan, override_font);
    mask_top_corners(&mut pixmap, width, plan.height, plan.corner_radius_top_left, plan.corner_radius_top_right);

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

    let Some(fonts) = titlebar_fonts(override_font) else {
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

    let rgba = premultiplied_rgba(plan.text_color, 1.0);
    let data = pixmap.data_mut();
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
                    || !titlebar_pixel_visible(
                        pixel_x,
                        pixel_y,
                        width,
                        plan.corner_radius_top_left,
                        plan.corner_radius_top_right,
                    )
                {
                    return;
                }

                blend_premultiplied_rgba(data, width, pixel_x, pixel_y, rgba, coverage);
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

        let x = if laid_out.is_empty() { 0.0 } else { current_width + letter_spacing };
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

fn titlebar_fonts(override_font: Option<&TitlebarFontConfig>) -> Option<TitlebarFonts> {
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

    TITLEBAR_FONTS.get_or_init(default_titlebar_fonts).as_ref().cloned()
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
    let cache = TITLEBAR_FONT_CACHE
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()));

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

fn premultiplied_rgba(color: spiders_scene::ColorValue, coverage: f32) -> [u8; 4] {
    let alpha = ((color.alpha as f32 / 255.0) * coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
    [
        premultiply(color.red, alpha),
        premultiply(color.green, alpha),
        premultiply(color.blue, alpha),
        alpha,
    ]
}

fn premultiply(component: u8, alpha: u8) -> u8 {
    ((u32::from(component) * u32::from(alpha) + 127) / 255) as u8
}

fn blend_premultiplied_rgba(
    data: &mut [u8],
    width: i32,
    x: i32,
    y: i32,
    source: [u8; 4],
    coverage: f32,
) {
    let source = premultiplied_rgba(
        spiders_scene::ColorValue {
            red: source[0],
            green: source[1],
            blue: source[2],
            alpha: source[3],
        },
        coverage,
    );
    let offset = ((y * width + x) * 4) as usize;
    if offset + 3 >= data.len() {
        return;
    }

    let dest = [data[offset], data[offset + 1], data[offset + 2], data[offset + 3]];
    let inv_alpha = 255u16 - u16::from(source[3]);

    data[offset] = (u16::from(source[0]) + ((u16::from(dest[0]) * inv_alpha + 127) / 255)) as u8;
    data[offset + 1] =
        (u16::from(source[1]) + ((u16::from(dest[1]) * inv_alpha + 127) / 255)) as u8;
    data[offset + 2] =
        (u16::from(source[2]) + ((u16::from(dest[2]) * inv_alpha + 127) / 255)) as u8;
    data[offset + 3] =
        (u16::from(source[3]) + ((u16::from(dest[3]) * inv_alpha + 127) / 255)) as u8;
}

fn mask_top_corners(
    pixmap: &mut Pixmap,
    width: i32,
    height: i32,
    corner_radius_top_left: i32,
    corner_radius_top_right: i32,
) {
    let data = pixmap.data_mut();
    for y in 0..height {
        for x in 0..width {
            if titlebar_pixel_visible(x, y, width, corner_radius_top_left, corner_radius_top_right) {
                continue;
            }

            let offset = ((y * width + x) * 4) as usize;
            if offset + 3 < data.len() {
                data[offset] = 0;
                data[offset + 1] = 0;
                data[offset + 2] = 0;
                data[offset + 3] = 0;
            }
        }
    }
}

fn titlebar_pixel_visible(
    x: i32,
    y: i32,
    width: i32,
    corner_radius_top_left: i32,
    corner_radius_top_right: i32,
) -> bool {
    let inside_corner = |x: i32, y: i32, origin_x: i32, radius: i32| {
        if radius <= 0 || y >= radius {
            return true;
        }
        let dx = x - origin_x;
        let dy = y - (radius - 1);
        dx * dx + dy * dy <= (radius - 1) * (radius - 1)
    };

    if corner_radius_top_left > 0 && x < corner_radius_top_left && y < corner_radius_top_left {
        return inside_corner(x, y, corner_radius_top_left - 1, corner_radius_top_left);
    }

    if corner_radius_top_right > 0 && x >= width - corner_radius_top_right && y < corner_radius_top_right {
        return inside_corner(x, y, width - corner_radius_top_right, corner_radius_top_right);
    }

    true
}