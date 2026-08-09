use dioxus::prelude::*;
use erpx_ui::kit::{
    Button, ButtonSize, ButtonVariant, NumberField, SecretField, SelectControl, TextArea,
    tooltip::{Tooltip, TooltipContent, TooltipTrigger},
};

#[component]
fn form_fixture() -> Element {
    rsx! {
        Button {
            variant: ButtonVariant::Destructive,
            size: ButtonSize::Sm,
            disabled: true,
            "Delete"
        }
        SecretField {
            value: "never-log-this-secret".to_owned(),
            name: "mfa".to_owned(),
            autocomplete: "one-time-code".to_owned(),
            required: true,
            aria_label: "Fresh MFA".to_owned(),
        }
        NumberField {
            value: "12".to_owned(),
            min: "1".to_owned(),
            max: "24".to_owned(),
            aria_label: "Duration".to_owned(),
        }
        TextArea {
            value: "Audited reason".to_owned(),
            maxlength: 240usize,
            aria_label: "Reason".to_owned(),
        }
        SelectControl { aria_label: "Scope",
            option { value: "platform", "Platform" }
        }
    }
}

#[component]
fn tooltip_fixture() -> Element {
    rsx! {
        Tooltip { default_open: true,
            TooltipTrigger { aria_disabled: "true", "Locked route" }
            TooltipContent { "Requires Finance Viewer" }
        }
    }
}

#[test]
fn kit_forms_render_stable_semantics_without_secret_data_attributes() {
    let mut dom = VirtualDom::new(form_fixture);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    for component in [
        "button",
        "secret-field",
        "number-field",
        "textarea",
        "select",
    ] {
        assert!(
            html.contains(&format!("data-erpx-component=\"{component}\"")),
            "missing semantic marker for {component}: {html}"
        );
    }
    assert!(html.contains("data-tone=\"destructive\""));
    assert!(html.contains("data-size=\"sm\""));
    assert!(html.contains("disabled"));

    for data_attribute in html
        .split_whitespace()
        .filter(|attribute| attribute.starts_with("data-"))
    {
        assert!(!data_attribute.contains("never-log-this-secret"));
    }
}

#[test]
fn tooltip_trigger_defaults_to_a_focusable_non_navigating_button() {
    let mut dom = VirtualDom::new(tooltip_fixture);
    dom.rebuild_in_place();
    let html = dioxus_ssr::render(&dom);
    assert!(
        html.contains("<button"),
        "tooltip trigger was not a button: {html}"
    );
    assert!(html.contains("type=\"button\""));
    assert!(html.contains("tabindex=\"0\""));
    assert!(html.contains("aria-disabled=\"true\""));
    assert!(html.contains("aria-describedby="));
    assert!(html.contains("role=\"tooltip\""));
    assert!(
        html.contains("Locked route"),
        "tooltip trigger dropped its visible children: {html}"
    );
    assert!(!html.contains("href="));
}

#[test]
fn vendored_source_is_exactly_pinned_and_records_local_patches() {
    let provenance = include_str!("../../../third_party/dioxus-components/ERPX-PATCH.md");
    assert!(provenance.contains("bf007c15d0cf4d04d3181cc46cf12325aa773955"));
    assert!(provenance.contains("SSR/hydration"));
}
