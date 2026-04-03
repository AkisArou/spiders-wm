use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSelectOption {
    pub value: String,
    pub label: String,
}

fn merge_classes(base: &str, extra: Option<&str>) -> String {
    match extra.filter(|value| !value.is_empty()) {
        Some(extra) => format!("{base} {extra}"),
        None => base.to_string(),
    }
}

#[component]
pub fn TerminalSelect(
    value: String,
    aria_label: String,
    options: Vec<TerminalSelectOption>,
    onchange: EventHandler<String>,
    class: Option<String>,
) -> Element {
    let classes = merge_classes("ui-select", class.as_deref());

    rsx! {
        div { class: "ui-select-wrap",
            select {
                class: "{classes}",
                value,
                aria_label,
                onchange: move |event| onchange.call(event.value()),

                for option in options {
                    option { key: "{option.value}", value: option.value.clone(), "{option.label}" }
                }
            }
        }
    }
}