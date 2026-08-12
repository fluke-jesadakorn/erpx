//! Controlled ERPX form composites built on the styled input primitives.

use dioxus::prelude::*;

use super::{input::Input, textarea::Textarea};

#[component]
pub fn SecretField(
    value: String,
    #[props(default)] on_value_change: Callback<String>,
    #[props(default)] name: String,
    #[props(default)] autocomplete: String,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] aria_describedby: Option<String>,
) -> Element {
    rsx! {
        Input {
            r#type: "password",
            value,
            name,
            autocomplete,
            disabled,
            required,
            aria_label,
            aria_describedby,
            "data-erpx-component": "secret-field",
            oninput: move |event: FormEvent| on_value_change.call(event.value()),
        }
    }
}

#[component]
pub fn NumberField(
    value: String,
    #[props(default)] on_value_change: Callback<String>,
    #[props(default)] name: String,
    #[props(default)] min: Option<String>,
    #[props(default)] max: Option<String>,
    #[props(default)] step: Option<String>,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] aria_describedby: Option<String>,
) -> Element {
    rsx! {
        Input {
            r#type: "number",
            value,
            name,
            min,
            max,
            step,
            disabled,
            required,
            aria_label,
            aria_describedby,
            "data-erpx-component": "number-field",
            oninput: move |event: FormEvent| on_value_change.call(event.value()),
        }
    }
}

#[component]
pub fn TextArea(
    value: String,
    #[props(default)] on_value_change: Callback<String>,
    #[props(default)] name: String,
    #[props(default)] disabled: bool,
    #[props(default)] required: bool,
    #[props(default)] maxlength: Option<usize>,
    #[props(default)] aria_label: Option<String>,
    #[props(default)] aria_describedby: Option<String>,
) -> Element {
    rsx! {
        Textarea {
            value,
            name,
            disabled,
            required,
            maxlength,
            aria_label,
            aria_describedby,
            "data-erpx-component": "textarea",
            oninput: move |event: FormEvent| on_value_change.call(event.value()),
        }
    }
}
