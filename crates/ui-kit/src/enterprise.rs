use dioxus::prelude::*;
use erpx_ui_core::{
    AdaptiveDensity, ClientFormFactor, EnterpriseState, EnterpriseStateKind, Locale, MutationMode,
    use_locale,
};

fn state_key(value: EnterpriseStateKind) -> &'static str {
    match value {
        EnterpriseStateKind::Loading => "loading",
        EnterpriseStateKind::Empty => "empty",
        EnterpriseStateKind::Denied => "denied",
        EnterpriseStateKind::Restricted => "restricted",
        EnterpriseStateKind::Stale => "stale",
        EnterpriseStateKind::Degraded => "degraded",
        EnterpriseStateKind::Conflict => "conflict",
        EnterpriseStateKind::Retry => "retry",
        EnterpriseStateKind::Success => "success",
    }
}

fn announcement_role(value: EnterpriseStateKind) -> &'static str {
    if matches!(
        value,
        EnterpriseStateKind::Denied
            | EnterpriseStateKind::Restricted
            | EnterpriseStateKind::Conflict
    ) {
        "alert"
    } else {
        "status"
    }
}

#[component]
pub fn EnterpriseStatePanel(
    state: EnterpriseState,
    #[props(default)] on_retry: Option<EventHandler<()>>,
) -> Element {
    let locale = use_locale().current();
    let retry_label = match locale {
        Locale::En => "Retry",
        Locale::Th => "ลองอีกครั้ง",
    };
    let key = state_key(state.kind);
    let role = announcement_role(state.kind);
    let payload_policy = if state.may_show_payload() {
        "bounded"
    } else {
        "none"
    };
    rsx! {
        section {
            class: "ui-enterprise-state",
            role,
            aria_live: if role == "alert" { "assertive" } else { "polite" },
            aria_atomic: "true",
            aria_busy: if state.kind == EnterpriseStateKind::Loading { "true" } else { "false" },
            "data-state": key,
            "data-payload-policy": payload_policy,
            tabindex: "-1",
            h2 { {state.title} }
            p { {state.detail} }
            if state.retryable {
                if let Some(handler) = on_retry {
                    button {
                        r#type: "button",
                        class: "ui-enterprise-retry",
                        onclick: move |_| handler.call(()),
                        {retry_label}
                    }
                }
            }
        }
    }
}

#[component]
pub fn AdaptiveCollectionSurface(
    form_factor: ClientFormFactor,
    mutation_mode: MutationMode,
    desktop: Element,
    mobile: Element,
    #[props(default)] bottom_actions: Option<Element>,
) -> Element {
    let locale = use_locale().current();
    let density = AdaptiveDensity::for_form_factor(form_factor);
    let density_key = match density {
        AdaptiveDensity::Dense => "dense",
        AdaptiveDensity::Comfortable => "comfortable",
    };
    let mutation_key = match mutation_mode {
        MutationMode::Enabled => "enabled",
        MutationMode::ReadOnlyOffline => "read-only-offline",
        MutationMode::Denied => "denied",
    };
    rsx! {
        section {
            class: "ui-adaptive-collection",
            "data-density": density_key,
            "data-mutation": mutation_key,
            div { class: "ui-adaptive-desktop", {desktop} }
            div { class: "ui-adaptive-mobile", {mobile} }
            if let Some(actions) = bottom_actions {
                nav {
                    class: "ui-mobile-bottom-actions",
                    aria_label: match locale { Locale::En => "Record actions", Locale::Th => "การทำงานกับรายการ" },
                    "data-enabled": if mutation_mode.may_submit() { "true" } else { "false" },
                    {actions}
                }
            }
            if mutation_mode == MutationMode::ReadOnlyOffline {
                p { class: "ui-offline-read-only", role: "status", aria_live: "polite", {match locale { Locale::En => "Offline: read-only. Changes are not queued.", Locale::Th => "ออฟไลน์: ดูได้อย่างเดียว ระบบจะไม่เข้าคิวการเปลี่ยนแปลง" }} }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_roles_preserve_authorization_and_recovery_priority() {
        assert_eq!(announcement_role(EnterpriseStateKind::Denied), "alert");
        assert_eq!(announcement_role(EnterpriseStateKind::Restricted), "alert");
        assert_eq!(announcement_role(EnterpriseStateKind::Stale), "status");
        assert_eq!(state_key(EnterpriseStateKind::Degraded), "degraded");
    }
}
