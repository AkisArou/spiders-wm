use clsx::clsx;
use leptos::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSelectOption {
    pub value: String,
    pub label: String,
}

#[component]
pub fn TerminalSelect(
    #[prop(into)] value: Signal<String>,
    #[prop(into)] aria_label: Oco<'static, str>,
    options: Vec<TerminalSelectOption>,
    onchange: Callback<String>,
    #[prop(optional, into)] class: String,
) -> impl IntoView {
    let classes = clsx!("ui-select", (!class.is_empty(), class.as_str()));

    view! {
        <div class="ui-select-wrap">
            <select
                class=classes
                prop:value=move || value.get()
                aria-label=aria_label
                on:change=move |event| onchange.run(event_target_value(&event))
            >
                {options
                    .into_iter()
                    .map(|option| {
                        let option_value = option.value.clone();
                        let option_label = option.label;

                        view! { <option value=option_value>{option_label}</option> }
                    })
                    .collect_view()}
            </select>
        </div>
    }
}
