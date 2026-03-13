#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssParserStrategy {
    CustomSubset,
    CssParser,
    LightningCss,
    Raffia,
    SwcCss,
}
