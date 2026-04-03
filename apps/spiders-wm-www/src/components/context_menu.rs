use clsx::clsx;
use leptos::html;
use leptos::prelude::*;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::HtmlElement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ContextMenuPosition {
    pub x: i32,
    pub y: i32,
}

#[component]
pub fn ContextMenu(
    children: Children,
    #[prop(into)] open: Signal<bool>,
    #[prop(into)] position: Signal<ContextMenuPosition>,
    on_close: Callback<()>,
) -> impl IntoView {
    let menu_ref = NodeRef::<html::Div>::new();

    Effect::new(move |_| {
        let is_open = open.get();
        let _ = position.get();

        let Some(menu) = menu_ref.get() else {
            return;
        };

        if is_open {
            if !popover_is_open(&menu) {
                call_popover_method(&menu, "showPopover");
            }
        } else if popover_is_open(&menu) {
            call_popover_method(&menu, "hidePopover");
        }
    });

    view! {
        <div
            node_ref=menu_ref
            popover="auto"
            tabindex="-1"
            class="inset-auto p-1 m-0 text-sm border outline-none min-w-40 border-terminal-border-strong bg-terminal-bg-subtle text-terminal-fg shadow-[0_12px_24px_rgba(0,0,0,0.35)]"
            style=move || {
                let position = position.get();
                format!("position: fixed; left: {}px; top: {}px;", position.x, position.y)
            }
            on:contextmenu=move |event| {
                event.prevent_default();
                event.stop_propagation();
            }
            on:toggle=move |_| {
                if let Some(menu) = menu_ref.get() {
                    if !popover_is_open(&menu) {
                        on_close.run(());
                    }
                }
            }
        >
            <div class="grid gap-0.5">{children()}</div>
        </div>
    }
}

#[component]
pub fn ContextMenuItem(
    #[prop(into)] label: Oco<'static, str>,
    on_select: Callback<()>,
    on_close: Callback<()>,
    #[prop(into)] disabled: Signal<bool>,
    #[prop(optional)] destructive: bool,
) -> impl IntoView {
    let class = move || {
        clsx!(
            "flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-sm transition-colors",
            (disabled.get(), "cursor-not-allowed text-terminal-faint opacity-50"),
            (!disabled.get() && destructive, "text-terminal-error hover:bg-terminal-error/10"),
            (!disabled.get() && !destructive, "text-terminal-fg hover:bg-terminal-bg-hover")
        )
    };

    view! {
        <button
            class=class
            type="button"
            disabled=move || disabled.get()
            on:click=move |_| {
                if disabled.get_untracked() {
                    return;
                }
                on_select.run(());
                on_close.run(());
            }
        >
            <span>{label}</span>
        </button>
    }
}

fn popover_is_open(element: &HtmlElement) -> bool {
    element.matches(":popover-open").unwrap_or(false)
}

fn call_popover_method(element: &HtmlElement, method_name: &str) {
    let Ok(method) = js_sys::Reflect::get(element.as_ref(), &JsValue::from_str(method_name)) else {
        return;
    };
    let Ok(method) = method.dyn_into::<js_sys::Function>() else {
        return;
    };

    let _ = method.call0(element.as_ref());
}
