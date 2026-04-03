use clsx::clsx;
use leptos::prelude::*;

#[component]
pub fn Panel(children: Children, #[prop(optional, into)] class: String) -> impl IntoView {
    let classes = clsx!(
        "flex min-h-0 flex-col overflow-hidden border border-terminal-border bg-terminal-bg-subtle",
        (!class.is_empty(), class.as_str())
    );

    view! {
        <section class=classes>{children()}</section>
    }
}

#[component]
pub fn PanelBar(children: Children, #[prop(optional, into)] class: String) -> impl IntoView {
    let classes = clsx!(
        "flex items-center justify-between border-b border-terminal-border bg-terminal-bg-bar px-2 py-1 text-xs text-terminal-dim",
        (!class.is_empty(), class.as_str())
    );

    view! {
        <div class=classes>{children()}</div>
    }
}
