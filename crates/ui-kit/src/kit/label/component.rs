use dioxus::prelude::*;
use dioxus_primitives::label::{self, LabelProps};
#[css_module("/src/kit/label/style.css")]
struct Styles;

#[component]
pub fn Label(props: LabelProps) -> Element {
    rsx! {
        label::Label {
            class: Styles::dx_label,
            "data-erpx-component": "label",
            html_for: props.html_for,
            attributes: props.attributes,
            {props.children}
        }
    }
}
