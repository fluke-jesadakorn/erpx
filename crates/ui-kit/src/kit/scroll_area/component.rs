use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{self, ScrollAreaProps};

#[component]
pub fn ScrollArea(props: ScrollAreaProps) -> Element {
    rsx! {
        scroll_area::ScrollArea {
            direction: props.direction,
            always_show_scrollbars: props.always_show_scrollbars,
            scroll_type: props.scroll_type,
            attributes: props.attributes,
            "data-erpx-component": "scroll-area",
            {props.children}
        }
    }
}
