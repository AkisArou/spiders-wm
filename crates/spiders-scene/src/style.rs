use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Display {
    Block,
    Flex,
    Grid,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoxSizingValue {
    BorderBox,
    ContentBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FlexDirectionValue {
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FlexWrapValue {
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PositionValue {
    Relative,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OverflowValue {
    Visible,
    Clip,
    Hidden,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppearanceValue {
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorValue {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BorderStyleValue {
    None,
    Solid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoxShadowValue {
    pub color: Option<ColorValue>,
    pub offset_x: i32,
    pub offset_y: i32,
    pub blur_radius: i32,
    pub spread_radius: i32,
    pub inset: bool,
}

pub type FontFamilyValue = Vec<String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextAlignValue {
    Left,
    Right,
    Center,
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextTransformValue {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontWeightValue {
    Normal,
    Bold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignmentValue {
    Start,
    End,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentAlignmentValue {
    Start,
    End,
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    SpaceBetween,
    SpaceEvenly,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LengthPercentage {
    Px(f32),
    Percent(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SizeValue {
    Auto,
    LengthPercentage(LengthPercentage),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridPlacementValue {
    Auto,
    Line(i16),
    NamedLine(String, i16),
    Span(u16),
    NamedSpan(String, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridAutoFlow {
    Row,
    Column,
    RowDense,
    ColumnDense,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line<T> {
    pub start: T,
    pub end: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size2<T> {
    pub width: T,
    pub height: T,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GridTrackValue {
    Auto,
    MinContent,
    MaxContent,
    LengthPercentage(LengthPercentage),
    Fraction(f32),
    FitContent(LengthPercentage),
    MinMax(GridTrackMinValue, GridTrackMaxValue),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridRepetitionCount {
    AutoFill,
    AutoFit,
    Count(u16),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridTrackRepeat {
    pub count: GridRepetitionCount,
    pub tracks: Vec<GridTrackValue>,
    pub line_names: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GridTemplateComponent {
    Single(GridTrackValue),
    Repeat(GridTrackRepeat),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridTemplate {
    pub components: Vec<GridTemplateComponent>,
    pub line_names: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridTemplateArea {
    pub name: String,
    pub row_start: u16,
    pub row_end: u16,
    pub column_start: u16,
    pub column_end: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GridTrackMinValue {
    Auto,
    MinContent,
    MaxContent,
    LengthPercentage(LengthPercentage),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum GridTrackMaxValue {
    Auto,
    MinContent,
    MaxContent,
    LengthPercentage(LengthPercentage),
    Fraction(f32),
    FitContent(LengthPercentage),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoxEdges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ComputedStyle {
    pub display: Option<Display>,
    pub box_sizing: Option<BoxSizingValue>,
    pub aspect_ratio: Option<f32>,
    pub appearance: Option<AppearanceValue>,
    pub background: Option<ColorValue>,
    pub color: Option<ColorValue>,
    pub opacity: Option<f32>,
    pub border_color: Option<ColorValue>,
    pub border_side_colors: Option<BoxEdges<Option<ColorValue>>>,
    pub border_style: Option<BoxEdges<BorderStyleValue>>,
    pub border_radius: Option<String>,
    pub box_shadow: Option<Vec<BoxShadowValue>>,
    pub backdrop_filter: Option<String>,
    pub transform: Option<String>,
    pub text_align: Option<TextAlignValue>,
    pub text_transform: Option<TextTransformValue>,
    pub font_family: Option<FontFamilyValue>,
    pub font_size: Option<LengthPercentage>,
    pub font_weight: Option<FontWeightValue>,
    pub letter_spacing: Option<f32>,
    pub animation: Option<String>,
    pub transition: Option<String>,
    pub transition_property: Option<String>,
    pub transition_duration: Option<String>,
    pub transition_timing_function: Option<String>,
    pub flex_direction: Option<FlexDirectionValue>,
    pub flex_wrap: Option<FlexWrapValue>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<SizeValue>,
    pub position: Option<PositionValue>,
    pub inset: Option<BoxEdges<SizeValue>>,
    pub overflow_x: Option<OverflowValue>,
    pub overflow_y: Option<OverflowValue>,
    pub width: Option<SizeValue>,
    pub height: Option<SizeValue>,
    pub min_width: Option<SizeValue>,
    pub min_height: Option<SizeValue>,
    pub max_width: Option<SizeValue>,
    pub max_height: Option<SizeValue>,
    pub align_items: Option<AlignmentValue>,
    pub align_self: Option<AlignmentValue>,
    pub justify_items: Option<AlignmentValue>,
    pub justify_self: Option<AlignmentValue>,
    pub align_content: Option<ContentAlignmentValue>,
    pub justify_content: Option<ContentAlignmentValue>,
    pub gap: Option<Size2<LengthPercentage>>,
    pub grid_template_rows: Option<GridTemplate>,
    pub grid_template_columns: Option<GridTemplate>,
    pub grid_auto_rows: Option<Vec<GridTrackValue>>,
    pub grid_auto_columns: Option<Vec<GridTrackValue>>,
    pub grid_auto_flow: Option<GridAutoFlow>,
    pub grid_template_areas: Option<Vec<GridTemplateArea>>,
    pub grid_row: Option<Line<GridPlacementValue>>,
    pub grid_column: Option<Line<GridPlacementValue>>,
    pub border: Option<BoxEdges<LengthPercentage>>,
    pub padding: Option<BoxEdges<LengthPercentage>>,
    pub margin: Option<BoxEdges<SizeValue>>,
}