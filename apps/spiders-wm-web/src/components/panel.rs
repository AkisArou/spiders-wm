use dioxus::prelude::*;

fn merge_classes(base: &str, extra: Option<&str>) -> String {
    match extra.filter(|value| !value.is_empty()) {
        Some(extra) => format!("{base} {extra}"),
        None => base.to_string(),
    }
}

#[component]
pub fn Panel(children: Element, class: Option<String>) -> Element {
    let classes = merge_classes(
        "flex min-h-0 flex-col overflow-hidden border border-terminal-border bg-terminal-bg-subtle",
        class.as_deref(),
    );

    rsx! {
        section { class: "{classes}", {children} }
    }
}

#[component]
pub fn PanelBar(children: Element, class: Option<String>) -> Element {
    let classes = merge_classes(
        "flex items-center justify-between border-b border-terminal-border bg-terminal-bg-bar px-2 py-1 text-xs text-terminal-dim",
        class.as_deref(),
    );

    rsx! {
        div { class: "{classes}", {children} }
    }
}