//! ERPX-owned semantic adapters for native elements that have no direct
//! upstream catalog equivalent, or whose option/row models are assembled by
//! domain composites. Keeping them here preserves one interactive DOM boundary.

use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

#[css_module("/src/kit/adapters.css")]
struct Styles;

#[component]
pub fn SelectControl(
    onchange: Option<EventHandler<FormEvent>>,
    oninput: Option<EventHandler<FormEvent>>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = select)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(select {
        class: Styles::erpx_select_control,
        "data-erpx-component": "select",
        "data-size": "default",
    });
    let attributes = merge_attributes(vec![base, attributes]);
    rsx! {
        select {
            onchange: move |event| _ = onchange.map(|callback| callback(event)),
            oninput: move |event| _ = oninput.map(|callback| callback(event)),
            ..attributes,
            {children}
        }
    }
}

/// Disclosure bridge for content whose view model is still authored as a
/// native summary/content pair. New surfaces should prefer `Collapsible` or
/// `Accordion`; this adapter preserves native keyboard and screen-reader
/// behavior during the atomic catalog cutover.
#[component]
pub fn Disclosure(
    #[props(extends = GlobalAttributes)]
    #[props(extends = details)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(details {
        class: Styles::erpx_disclosure,
        "data-erpx-component": "collapsible",
    });
    let attributes = merge_attributes(vec![base, attributes]);
    rsx! { details { ..attributes, {children} } }
}

/// ERPX DataTable DOM root. Sorting, selection, pagination, virtualization,
/// and transport callbacks remain in the bounded DataTable composite.
#[component]
pub fn Table(
    #[props(extends = GlobalAttributes)]
    #[props(extends = table)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(table {
        class: Styles::erpx_table,
        "data-erpx-component": "data-table",
    });
    let attributes = merge_attributes(vec![base, attributes]);
    rsx! { table { ..attributes, {children} } }
}

#[component]
pub fn Link(
    onclick: Option<EventHandler<MouseEvent>>,
    #[props(extends = GlobalAttributes)]
    #[props(extends = a)]
    attributes: Vec<Attribute>,
    children: Element,
) -> Element {
    let base = attributes!(a {
        class: Styles::erpx_link,
        "data-erpx-component": "link",
    });
    let attributes = merge_attributes(vec![base, attributes]);
    rsx! {
        a {
            onclick: move |event| _ = onclick.map(|callback| callback(event)),
            ..attributes,
            {children}
        }
    }
}
