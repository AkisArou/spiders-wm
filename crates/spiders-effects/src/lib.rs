pub mod model;

pub fn crate_ready() -> bool {
    true
}

pub use model::{
    compute_effect_style, effect_selector_matches, matching_effect_rules, parse_effect_stylesheet,
    Appearance, CompiledEffectDeclaration, EffectDeclaration, EffectPseudoElement,
    EffectPseudoState, EffectSelector, EffectSelectorSubject, EffectStyle, EffectStyleRule,
    EffectStyleSheet, EffectTarget, EffectsCssParseError, EffectsCssValueError, MatchedEffectRule,
    TitlebarEffects, WindowEffects,
};
