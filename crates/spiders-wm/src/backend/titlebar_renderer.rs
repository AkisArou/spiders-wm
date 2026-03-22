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

fn titlebar_fonts(
    font_family: Option<&str>,
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

    TITLEBAR_FONTS.get_or_init(default_titlebar_fonts).as_ref().cloned()
}

fn resolve_fonts_for_family(font_family: &str) -> Option<TitlebarFonts> {
    for family in parse_font_family_list(font_family) {
        let normalized = normalize_font_family_name(&family);
        let fonts = match normalized.as_str() {
            "serif" | "dejavu serif" | "liberation serif" => Some((
                SERIF_REGULAR_FONT_PATHS,
                SERIF_BOLD_FONT_PATHS,
            )),
            "monospace" | "dejavu sans mono" | "liberation mono" => Some((
                MONO_REGULAR_FONT_PATHS,
                MONO_BOLD_FONT_PATHS,
            )),
            "sans-serif" | "dejavu sans" | "liberation sans" | "system-ui" => Some((
                REGULAR_FONT_PATHS,
                BOLD_FONT_PATHS,
            )),
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

fn parse_font_family_list(font_family: &str) -> Vec<String> {
    let mut families = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for character in font_family.chars() {
        match character {
            '\'' | '"' => {
                if quote == Some(character) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(character);
                }
                current.push(character);
            }
            ',' if quote.is_none() => {
                let family = current.trim();
                if !family.is_empty() {
                    families.push(family.to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }

    let family = current.trim();
    if !family.is_empty() {
        families.push(family.to_string());
    }

    families
}

fn normalize_font_family_name(family: &str) -> String {
    family
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase()
}

fn draw_box_shadow(pixmap: &mut Pixmap, width: i32, plan: &TitlebarPlan) {
    let Some(shadow) = plan.box_shadow.as_deref().and_then(parse_box_shadow) else {
        return;
    };
    if shadow.blur_radius <= 0 && shadow.spread_radius <= 0 {
        return;
    }

    let steps = shadow.blur_radius.max(1).min(16);
    let outer_left = shadow.offset_x - shadow.spread_radius;
    let outer_top = shadow.offset_y - shadow.spread_radius;
    let outer_width = width + shadow.spread_radius * 2;
    let outer_height = plan.height + shadow.spread_radius * 2;
    let inner_left = shadow.offset_x;
    let inner_top = shadow.offset_y;

    for step in (0..steps).rev() {
        let inset = step;
        let alpha_scale = (steps - step) as f32 / steps as f32;
        let top = outer_top + inset;
        let left = outer_left + inset;
        let right = left + outer_width - inset * 2;
        let bottom = top + outer_height - inset * 2;

        if right <= left || bottom <= top {
            continue;
        }

        let alpha = (f32::from(shadow.color.alpha) * 0.22 * alpha_scale).round() as u8;
        if alpha == 0 {
            continue;
        }

        let color = Color::from_rgba8(shadow.color.red, shadow.color.green, shadow.color.blue, alpha);
        fill_shadow_band(pixmap, left, top, right - left, (inner_top - top).max(0), color);
        fill_shadow_band(
            pixmap,
            left,
            (inner_top + plan.height).min(bottom),
            right - left,
            (bottom - (inner_top + plan.height)).max(0),
            color,
        );
        fill_shadow_band(
            pixmap,
            left,
            inner_top.max(top),
            (inner_left - left).max(0),
            (plan.height).min(bottom - inner_top.max(top)),
            color,
        );
        fill_shadow_band(
            pixmap,
            (inner_left + width).min(right),
            inner_top.max(top),
            (right - (inner_left + width)).max(0),
            (plan.height).min(bottom - inner_top.max(top)),
            color,
        );
    }
}

fn fill_shadow_band(
    pixmap: &mut Pixmap,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    color: Color,
) {
    if width <= 0 || height <= 0 {
        return;
    }

    let Some(rect) = Rect::from_xywh(x as f32, y as f32, width as f32, height as f32) else {
        return;
    };

    let mut paint = Paint::default();
    paint.set_color(color);
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParsedBoxShadow {
    offset_x: i32,
    offset_y: i32,
    blur_radius: i32,
    spread_radius: i32,
    color: spiders_scene::ColorValue,
}

fn parse_box_shadow(raw: &str) -> Option<ParsedBoxShadow> {
    let tokens = split_shadow_tokens(split_first_shadow(raw).trim());
    if tokens.iter().any(|token| token.eq_ignore_ascii_case("inset")) {
        return None;
    }

    let mut lengths = Vec::new();
    let mut color = None;
    for token in tokens {
        if let Some(parsed_color) = parse_shadow_color(&token) {
            color = Some(parsed_color);
            continue;
        }
        let length = parse_shadow_length(&token)?;
        lengths.push(length);
    }

    let [offset_x, offset_y, rest @ ..] = lengths.as_slice() else {
        return None;
    };

    Some(ParsedBoxShadow {
        offset_x: *offset_x,
        offset_y: *offset_y,
        blur_radius: *rest.first().unwrap_or(&0),
        spread_radius: *rest.get(1).unwrap_or(&0),
        color: color.unwrap_or(spiders_scene::ColorValue {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 96,
        }),
    })
}

fn split_first_shadow(raw: &str) -> &str {
    let mut paren_depth = 0;
    for (index, character) in raw.char_indices() {
        match character {
            '(' => paren_depth += 1,
            ')' => paren_depth = (paren_depth - 1).max(0),
            ',' if paren_depth == 0 => return &raw[..index],
            _ => {}
        }
    }

    raw
}

fn split_shadow_tokens(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for character in raw.chars() {
        match character {
            '(' => {
                paren_depth += 1;
                current.push(character);
            }
            ')' => {
                paren_depth = (paren_depth - 1).max(0);
                current.push(character);
            }
            c if c.is_whitespace() && paren_depth == 0 => {
                let token = current.trim();
                if !token.is_empty() {
                    tokens.push(token.to_string());
                }
                current.clear();
            }
            _ => current.push(character),
        }
    }

    let token = current.trim();
    if !token.is_empty() {
        tokens.push(token.to_string());
    }

    tokens
}

fn parse_shadow_length(token: &str) -> Option<i32> {
    match token {
        "0" | "0.0" => Some(0),
        _ => token
            .strip_suffix("px")
            .and_then(|value| value.parse::<f32>().ok())
            .map(|value| value.round() as i32),
    }
}

fn parse_shadow_color(token: &str) -> Option<spiders_scene::ColorValue> {
    let token = token.trim();
    if token.starts_with('#') {
        return parse_hex_color(token);
    }
    if token.starts_with("rgba(") && token.ends_with(')') {
        return parse_rgba_color(token);
    }
    if token.starts_with("rgb(") && token.ends_with(')') {
        return parse_rgb_color(token);
    }
    None
}

fn parse_hex_color(token: &str) -> Option<spiders_scene::ColorValue> {
    let hex = token.strip_prefix('#')?;
    match hex.len() {
        6 => Some(spiders_scene::ColorValue {
            red: u8::from_str_radix(&hex[0..2], 16).ok()?,
            green: u8::from_str_radix(&hex[2..4], 16).ok()?,
            blue: u8::from_str_radix(&hex[4..6], 16).ok()?,
            alpha: 255,
        }),
        8 => Some(spiders_scene::ColorValue {
            red: u8::from_str_radix(&hex[0..2], 16).ok()?,
            green: u8::from_str_radix(&hex[2..4], 16).ok()?,
            blue: u8::from_str_radix(&hex[4..6], 16).ok()?,
            alpha: u8::from_str_radix(&hex[6..8], 16).ok()?,
        }),
        _ => None,
    }
}

fn parse_rgb_color(token: &str) -> Option<spiders_scene::ColorValue> {
    let inner = token.strip_prefix("rgb(")?.strip_suffix(')')?;
    let values = inner
        .split(',')
        .map(|part| part.trim().parse::<u8>().ok())
        .collect::<Option<Vec<_>>>()?;
    let [red, green, blue] = values.as_slice() else {
        return None;
    };
    Some(spiders_scene::ColorValue {
        red: *red,
        green: *green,
        blue: *blue,
        alpha: 255,
    })
}

fn parse_rgba_color(token: &str) -> Option<spiders_scene::ColorValue> {
    let inner = token.strip_prefix("rgba(")?.strip_suffix(')')?;
    let values = inner.split(',').map(|part| part.trim()).collect::<Vec<_>>();
    let [red, green, blue, alpha] = values.as_slice() else {
        return None;
    };
    let alpha = alpha.parse::<f32>().ok()?.clamp(0.0, 1.0);
    Some(spiders_scene::ColorValue {
        red: red.parse::<u8>().ok()?,
        green: green.parse::<u8>().ok()?,
        blue: blue.parse::<u8>().ok()?,
        alpha: (alpha * 255.0).round() as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_font_family_lists() {
        assert_eq!(
            parse_font_family_list("'DejaVu Sans', sans-serif, monospace"),
            vec!["'DejaVu Sans'", "sans-serif", "monospace"]
        );
        assert_eq!(normalize_font_family_name(" 'DejaVu Sans' "), "dejavu sans");
    }

    #[test]
    fn parses_supported_box_shadow_values() {
        assert_eq!(
            parse_box_shadow("0 3px 8px rgba(0, 0, 0, 0.35)"),
            Some(ParsedBoxShadow {
                offset_x: 0,
                offset_y: 3,
                blur_radius: 8,
                spread_radius: 0,
                color: spiders_scene::ColorValue {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 89,
                },
            })
        );
        assert!(parse_box_shadow("inset 0 2px 4px #000000").is_none());
        assert_eq!(
            split_first_shadow("0 3px 8px rgba(0, 0, 0, 0.35), 0 1px 2px #000000"),
            "0 3px 8px rgba(0, 0, 0, 0.35)"
        );
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