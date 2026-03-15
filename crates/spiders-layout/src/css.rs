mod domain;
mod syntax;
mod taffy;

pub use domain::*;
pub use syntax::{parse_stylesheet, CssParseError};
pub use taffy::{
    compile_declaration, compute_style, map_computed_style_to_taffy, matching_rules,
    selector_matches, CompiledDeclaration, CssValueError, NodeComputedStyle, StyledLayoutTree,
};

#[cfg(test)]
mod tests {
    use super::*;
    use spiders_shared::ids::WindowId;
    use spiders_shared::layout::{LayoutNodeMeta, ResolvedLayoutNode};

    fn runtime_window_with_meta(meta: LayoutNodeMeta) -> ResolvedLayoutNode {
        ResolvedLayoutNode::Window {
            meta,
            window_id: Some(WindowId::from("win-1")),
        }
    }

    fn only_declaration(source: &str) -> Declaration {
        parse_stylesheet(source)
            .unwrap()
            .rules
            .into_iter()
            .next()
            .unwrap()
            .declarations
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn parses_basic_rule_with_multiple_selectors() {
        let sheet =
            parse_stylesheet("workspace, .stack { display: flex; flex-direction: row; gap: 8px; }")
                .unwrap();

        assert_eq!(
            sheet,
            StyleSheet {
                rules: vec![StyleRule {
                    selectors: vec![
                        Selector::Type(NodeSelector::Workspace),
                        Selector::Class("stack".into()),
                    ],
                    declarations: {
                        let mut sheet = parse_stylesheet(
                            "window { display: flex; flex-direction: row; gap: 8px; }",
                        )
                        .unwrap();
                        sheet.rules.remove(0).declarations
                    },
                }],
            }
        );
    }

    #[test]
    fn parses_id_selector() {
        let sheet = parse_stylesheet("#main { width: 50%; }").unwrap();

        assert_eq!(sheet.rules[0].selectors, vec![Selector::Id("main".into())]);
    }

    #[test]
    fn parses_attribute_selector() {
        let selector = parse_stylesheet("window[app_id=\"foot\"] { width: 100%; }")
            .unwrap()
            .rules[0]
            .selectors[0]
            .clone();

        assert_eq!(
            selector,
            Selector::Attribute(AttributeSelector {
                name: "app_id".into(),
                value: "foot".into(),
            })
        );
    }

    #[test]
    fn rejects_unsupported_selector() {
        let error = parse_stylesheet("slot { display: flex; }").unwrap_err();

        assert_eq!(
            error,
            CssParseError::UnsupportedSelector {
                selector: "slot".into(),
            }
        );
    }

    #[test]
    fn rejects_unsupported_property() {
        let error = parse_stylesheet("window { color: red; }").unwrap_err();

        assert_eq!(
            error,
            CssParseError::UnsupportedProperty {
                property: "color".into(),
            }
        );
    }

    #[test]
    fn rejects_at_rules_for_v1() {
        let error = parse_stylesheet("@media screen { window { width: 100%; } }").unwrap_err();

        assert_eq!(
            error,
            CssParseError::UnsupportedAtRule {
                name: "media".into(),
            }
        );
    }

    #[test]
    fn matches_type_id_and_class_selectors_against_runtime_nodes() {
        let node = runtime_window_with_meta(LayoutNodeMeta {
            id: Some("main".into()),
            class: vec!["stack".into(), "focused".into()],
            data: [("app_id".into(), "foot".into())].into_iter().collect(),
            ..LayoutNodeMeta::default()
        });

        assert!(selector_matches(
            &Selector::Type(NodeSelector::Window),
            &node
        ));
        assert!(selector_matches(&Selector::Id("main".into()), &node));
        assert!(selector_matches(&Selector::Class("stack".into()), &node));
        assert!(selector_matches(
            &Selector::Attribute(AttributeSelector {
                name: "app_id".into(),
                value: "foot".into(),
            }),
            &node,
        ));
        assert!(!selector_matches(
            &Selector::Type(NodeSelector::Group),
            &node
        ));
        assert!(!selector_matches(&Selector::Class("missing".into()), &node));
    }

    #[test]
    fn collects_rules_matching_any_selector_in_rule() {
        let sheet = parse_stylesheet(
            "group { gap: 8px; } #main, .stack { width: 50%; } window { height: 100%; }",
        )
        .unwrap();
        let node = runtime_window_with_meta(LayoutNodeMeta {
            id: Some("main".into()),
            class: vec!["stack".into()],
            ..LayoutNodeMeta::default()
        });

        let matches = matching_rules(&sheet, &node);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].rule_index, 1);
        assert_eq!(matches[1].rule_index, 2);
        assert_eq!(matches[0].rule.declarations[0].property, "width");
        assert_eq!(matches[1].rule.declarations[0].property, "height");
    }

    #[test]
    fn compiles_typed_declaration_values() {
        let declaration = only_declaration("window { padding: 8px 16px; }");

        let compiled = compile_declaration(&declaration).unwrap();

        assert_eq!(
            compiled,
            CompiledDeclaration::Padding(BoxEdges {
                top: LengthPercentage::Px(8.0),
                right: LengthPercentage::Px(16.0),
                bottom: LengthPercentage::Px(8.0),
                left: LengthPercentage::Px(16.0),
            })
        );
    }

    #[test]
    fn supports_display_none_aspect_ratio_and_two_axis_gap() {
        let sheet = parse_stylesheet(
            "window { display: none; aspect-ratio: 16 / 9; gap: 10px 20px; box-sizing: content-box; margin: auto 8px; }",
        )
        .unwrap();
        let node = runtime_window_with_meta(LayoutNodeMeta::default());

        let style = compute_style(&sheet, &node).unwrap();

        assert_eq!(style.display, Some(Display::None));
        assert_eq!(style.aspect_ratio, Some(16.0 / 9.0));
        assert_eq!(
            style.gap,
            Some(Size2 {
                width: LengthPercentage::Px(20.0),
                height: LengthPercentage::Px(10.0),
            })
        );
        assert_eq!(style.box_sizing, Some(BoxSizingValue::ContentBox));
        assert_eq!(
            style.margin,
            Some(BoxEdges {
                top: SizeValue::Auto,
                right: SizeValue::LengthPercentage(LengthPercentage::Px(8.0)),
                bottom: SizeValue::Auto,
                left: SizeValue::LengthPercentage(LengthPercentage::Px(8.0)),
            })
        );
    }

    #[test]
    fn supports_row_and_column_gap_overrides() {
        let sheet =
            parse_stylesheet("window { gap: 4px; row-gap: 12px; column-gap: 24px; }").unwrap();
        let node = runtime_window_with_meta(LayoutNodeMeta::default());

        let style = compute_style(&sheet, &node).unwrap();

        assert_eq!(
            style.gap,
            Some(Size2 {
                width: LengthPercentage::Px(24.0),
                height: LengthPercentage::Px(12.0),
            })
        );
    }

    #[test]
    fn supports_unitless_zero_for_size_values() {
        let sheet =
            parse_stylesheet("window { flex-basis: 0; min-width: 0; min-height: 0; padding: 0; }")
                .unwrap();
        let node = runtime_window_with_meta(LayoutNodeMeta::default());

        let style = compute_style(&sheet, &node).unwrap();

        assert_eq!(
            style.flex_basis,
            Some(SizeValue::LengthPercentage(LengthPercentage::Px(0.0)))
        );
        assert_eq!(
            style.min_width,
            Some(SizeValue::LengthPercentage(LengthPercentage::Px(0.0)))
        );
        assert_eq!(
            style.min_height,
            Some(SizeValue::LengthPercentage(LengthPercentage::Px(0.0)))
        );
        assert_eq!(
            style.padding,
            Some(BoxEdges {
                top: LengthPercentage::Px(0.0),
                right: LengthPercentage::Px(0.0),
                bottom: LengthPercentage::Px(0.0),
                left: LengthPercentage::Px(0.0),
            })
        );
    }

    #[test]
    fn later_matching_rules_override_earlier_declarations() {
        let sheet = parse_stylesheet(
            "window { width: 40%; gap: 8px; } .stack { width: 60%; } #main { gap: 12px; }",
        )
        .unwrap();
        let node = runtime_window_with_meta(LayoutNodeMeta {
            id: Some("main".into()),
            class: vec!["stack".into()],
            ..LayoutNodeMeta::default()
        });

        let style = compute_style(&sheet, &node).unwrap();

        assert_eq!(
            style.width,
            Some(SizeValue::LengthPercentage(LengthPercentage::Percent(60.0)))
        );
        assert_eq!(
            style.gap,
            Some(Size2 {
                width: LengthPercentage::Px(12.0),
                height: LengthPercentage::Px(12.0),
            })
        );
    }

    #[test]
    fn invalid_supported_property_value_fails_during_compilation() {
        let declaration = only_declaration("window { display: inline; }");

        let error = compile_declaration(&declaration).unwrap_err();

        assert_eq!(
            error,
            CssValueError::UnsupportedValue {
                property: "display".into(),
                value: "inline".into(),
            }
        );
    }

    #[test]
    fn compiles_grid_track_and_placement_values() {
        let tracks = compile_declaration(&only_declaration(
            "window { grid-template-columns: [left] 1fr repeat(2, [mid] 500px) minmax(100px, 2fr) [right]; }",
        ))
        .unwrap();
        let placement = compile_declaration(&only_declaration(
            "window { grid-column: left / span 2 right; }",
        ))
        .unwrap();

        assert_eq!(
            tracks,
            CompiledDeclaration::GridTemplateColumns(GridTemplate {
                components: vec![
                    GridTemplateComponent::Single(GridTrackValue::Fraction(1.0)),
                    GridTemplateComponent::Repeat(GridTrackRepeat {
                        count: GridRepetitionCount::Count(2),
                        tracks: vec![GridTrackValue::LengthPercentage(LengthPercentage::Px(
                            500.0
                        ))],
                        line_names: vec![vec!["mid".into()], vec![]],
                    }),
                    GridTemplateComponent::Single(GridTrackValue::MinMax(
                        GridTrackMinValue::LengthPercentage(LengthPercentage::Px(100.0)),
                        GridTrackMaxValue::Fraction(2.0),
                    )),
                ],
                line_names: vec![vec!["left".into()], vec![], vec![], vec!["right".into()],],
            })
        );
        assert_eq!(
            placement,
            CompiledDeclaration::GridColumn(Line {
                start: GridPlacementValue::NamedLine("left".into(), 1),
                end: GridPlacementValue::NamedSpan("right".into(), 2),
            })
        );
    }

    #[test]
    fn compiles_grid_template_areas() {
        let areas = compile_declaration(&only_declaration(
            "window { grid-template-areas: \"nav nav\" \"main side\"; }",
        ))
        .unwrap();

        assert_eq!(
            areas,
            CompiledDeclaration::GridTemplateAreas(vec![
                GridTemplateArea {
                    name: "main".into(),
                    row_start: 2,
                    row_end: 3,
                    column_start: 1,
                    column_end: 2,
                },
                GridTemplateArea {
                    name: "nav".into(),
                    row_start: 1,
                    row_end: 2,
                    column_start: 1,
                    column_end: 3,
                },
                GridTemplateArea {
                    name: "side".into(),
                    row_start: 2,
                    row_end: 3,
                    column_start: 2,
                    column_end: 3,
                },
            ])
        );
    }

    #[test]
    fn maps_grid_style_into_taffy_style() {
        let style = ComputedStyle {
            display: Some(Display::Grid),
            grid_template_columns: Some(GridTemplate {
                components: vec![
                    GridTemplateComponent::Single(GridTrackValue::Fraction(1.0)),
                    GridTemplateComponent::Repeat(GridTrackRepeat {
                        count: GridRepetitionCount::Count(2),
                        tracks: vec![GridTrackValue::LengthPercentage(LengthPercentage::Px(
                            500.0,
                        ))],
                        line_names: vec![vec!["mid".into()], vec![]],
                    }),
                ],
                line_names: vec![vec!["left".into()], vec![], vec![]],
            }),
            grid_template_areas: Some(vec![GridTemplateArea {
                name: "hero".into(),
                row_start: 1,
                row_end: 2,
                column_start: 1,
                column_end: 3,
            }]),
            grid_column: Some(Line {
                start: GridPlacementValue::NamedLine("left".into(), 1),
                end: GridPlacementValue::Auto,
            }),
            ..ComputedStyle::default()
        };

        let mapped = map_computed_style_to_taffy(&style);

        assert_eq!(mapped.display, ::taffy::prelude::Display::Grid);
        assert_eq!(mapped.grid_template_columns.len(), 2);
        assert_eq!(mapped.grid_template_column_names[0][0], "left");
        assert_eq!(mapped.grid_template_areas[0].name, "hero");
        assert_eq!(
            mapped.grid_column.start,
            ::taffy::prelude::GridPlacement::NamedLine("left".into(), 1)
        );
    }

    #[test]
    fn maps_computed_style_into_taffy_style() {
        let style = ComputedStyle {
            display: Some(Display::Flex),
            flex_direction: Some(FlexDirectionValue::Column),
            width: Some(SizeValue::LengthPercentage(LengthPercentage::Percent(60.0))),
            height: Some(SizeValue::LengthPercentage(LengthPercentage::Px(200.0))),
            gap: Some(Size2 {
                width: LengthPercentage::Px(12.0),
                height: LengthPercentage::Px(12.0),
            }),
            padding: Some(BoxEdges {
                top: LengthPercentage::Px(8.0),
                right: LengthPercentage::Px(16.0),
                bottom: LengthPercentage::Px(8.0),
                left: LengthPercentage::Px(16.0),
            }),
            ..ComputedStyle::default()
        };

        let mapped = map_computed_style_to_taffy(&style);

        assert_eq!(mapped.display, ::taffy::prelude::Display::Flex);
        assert_eq!(
            mapped.flex_direction,
            ::taffy::prelude::FlexDirection::Column
        );
        assert_eq!(mapped.size.width, ::taffy::prelude::Dimension::percent(0.6));
        assert_eq!(
            mapped.size.height,
            ::taffy::prelude::Dimension::length(200.0)
        );
        assert_eq!(
            mapped.gap.width,
            ::taffy::style::LengthPercentage::length(12.0)
        );
        assert_eq!(
            mapped.padding.left,
            ::taffy::style::LengthPercentage::length(16.0)
        );
    }
}
