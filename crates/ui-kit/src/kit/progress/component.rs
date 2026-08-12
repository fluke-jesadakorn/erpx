use dioxus::prelude::*;
use dioxus_primitives::progress::{self, ProgressProps};

#[css_module("/src/kit/progress/style.css")]
struct Styles;

#[component]
pub fn Progress(props: ProgressProps) -> Element {
    rsx! {
        progress::Progress {
            class: Styles::dx_progress,
            "data-erpx-component": "progress",
            value: props.value,
            max: props.max,
            attributes: props.attributes,
            progress::ProgressIndicator { class: Styles::dx_progress_indicator }
        }
    }
}
