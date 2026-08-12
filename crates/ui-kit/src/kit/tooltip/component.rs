use dioxus::prelude::*;
use dioxus_primitives::dioxus_attributes::attributes;
use dioxus_primitives::merge_attributes;
use dioxus_primitives::tooltip::{self, TooltipContentProps, TooltipProps};

#[css_module("/src/kit/tooltip/style.css")]
struct Styles;

#[component]
pub fn Tooltip(props: TooltipProps) -> Element {
    let base = attributes!(div {
        class: Styles::dx_tooltip,
        "data-erpx-component": "tooltip",
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        tooltip::Tooltip {
            disabled: props.disabled,
            open: props.open,
            default_open: props.default_open,
            on_open_change: props.on_open_change,
            attributes: merged,
            {props.children}
        }
    }
}

#[component]
pub fn TooltipTrigger(
    id: Option<String>,
    r#as: Option<Callback<Vec<Attribute>, Element>>,
    #[props(extends = GlobalAttributes)] attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let base = attributes!(button {
        class: Styles::dx_tooltip_trigger,
        onclick: move |event| if let Some(handler) = &onclick {
            handler.call(event);
        },
    });
    let merged = merge_attributes(vec![base, attributes]);
    let trigger_children = children.clone();
    let trigger = r#as.unwrap_or_else(|| {
        Callback::new(move |attributes: Vec<Attribute>| {
            rsx! {
                button {
                    r#type: "button",
                    ..attributes,
                    {trigger_children.clone()}
                }
            }
        })
    });

    rsx! {
        tooltip::TooltipTrigger {
            id,
            as: trigger,
            attributes: merged,
            {children}
        }
    }
}

#[component]
pub fn TooltipContent(props: TooltipContentProps) -> Element {
    let base = attributes!(div {
        class: Styles::dx_tooltip_content,
    });
    let merged = merge_attributes(vec![base, props.attributes]);

    rsx! {
        tooltip::TooltipContent {
            id: props.id,
            side: props.side,
            align: props.align,
            attributes: merged,
            {props.children}
        }
    }
}
