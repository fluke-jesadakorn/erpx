use dioxus::prelude::*;
use erpx_ui_core::Locale;
use erpx_ui_kit::kit::{Button as KitButton, ButtonVariant, Input as KitInput, Link as KitLink};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IdentityRecordKind {
    Principal,
    Role,
    Grant,
    AccessReview,
    BreakGlassSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityPrincipalLifecycleAction {
    Activate,
    Suspend,
    Reactivate,
    Disable,
}

impl IdentityPrincipalLifecycleAction {
    pub const fn capability(self) -> &'static str {
        match self {
            Self::Activate => "ActivatePrincipal",
            Self::Suspend => "SuspendPrincipal",
            Self::Reactivate => "ReactivatePrincipal",
            Self::Disable => "DisablePrincipal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityPrincipalCommandIntent {
    pub principal_id: String,
    pub action: IdentityPrincipalLifecycleAction,
    pub idempotency_key: String,
    pub expected_version: u64,
    pub purpose: String,
    pub mfa_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityCommandFeedback {
    pub state: String,
    pub message: String,
    pub audit_reference: Option<String>,
}

impl IdentityRecordKind {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Principal => "principal",
            Self::Role => "role",
            Self::Grant => "grant",
            Self::AccessReview => "access-review",
            Self::BreakGlassSession => "break-glass-session",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityProjectionEvidence {
    pub availability: String,
    pub freshness: String,
    pub measured_at: Option<String>,
    pub cursor: Option<String>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityField {
    pub label: String,
    pub value: String,
    pub redacted: bool,
    pub redaction_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityRecord {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub status: String,
    pub version: u64,
    pub measured_at: Option<String>,
    pub fields: Vec<IdentityField>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityAccessPage {
    pub kind: IdentityRecordKind,
    pub values: Vec<IdentityRecord>,
    pub next_cursor: Option<String>,
    pub page_size: u16,
    pub evidence: IdentityProjectionEvidence,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityAccessDetail {
    pub kind: IdentityRecordKind,
    pub record: IdentityRecord,
    pub evidence: IdentityProjectionEvidence,
}

fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

fn title(kind: IdentityRecordKind, locale: Locale) -> &'static str {
    match (kind, locale) {
        (IdentityRecordKind::Principal, Locale::En) => "Principals",
        (IdentityRecordKind::Principal, Locale::Th) => "ตัวตนหลัก",
        (IdentityRecordKind::Role, Locale::En) => "Roles",
        (IdentityRecordKind::Role, Locale::Th) => "บทบาท",
        (IdentityRecordKind::Grant, Locale::En) => "Grants",
        (IdentityRecordKind::Grant, Locale::Th) => "สิทธิ์ที่มอบหมาย",
        (IdentityRecordKind::AccessReview, Locale::En) => "Access reviews",
        (IdentityRecordKind::AccessReview, Locale::Th) => "การทบทวนสิทธิ์",
        (IdentityRecordKind::BreakGlassSession, Locale::En) => "Break-glass sessions",
        (IdentityRecordKind::BreakGlassSession, Locale::Th) => "เซสชันฉุกเฉิน",
    }
}

fn action_copy(action: IdentityPrincipalLifecycleAction, locale: Locale) -> &'static str {
    match (action, locale) {
        (IdentityPrincipalLifecycleAction::Activate, Locale::En) => "Activate",
        (IdentityPrincipalLifecycleAction::Activate, Locale::Th) => "เปิดใช้งาน",
        (IdentityPrincipalLifecycleAction::Suspend, Locale::En) => "Suspend",
        (IdentityPrincipalLifecycleAction::Suspend, Locale::Th) => "ระงับ",
        (IdentityPrincipalLifecycleAction::Reactivate, Locale::En) => "Reactivate",
        (IdentityPrincipalLifecycleAction::Reactivate, Locale::Th) => "เปิดใช้งานอีกครั้ง",
        (IdentityPrincipalLifecycleAction::Disable, Locale::En) => "Disable",
        (IdentityPrincipalLifecycleAction::Disable, Locale::Th) => "ปิดใช้งานถาวร",
    }
}

fn available_principal_actions(status: &str) -> &'static [IdentityPrincipalLifecycleAction] {
    use IdentityPrincipalLifecycleAction::{Activate, Disable, Reactivate, Suspend};
    match status {
        "invited" => &[Activate, Disable],
        "active" => &[Suspend, Disable],
        "suspended" => &[Reactivate, Disable],
        _ => &[],
    }
}

fn principal_command_idempotency_key(
    principal_id: &str,
    action: IdentityPrincipalLifecycleAction,
    expected_version: u64,
) -> String {
    // A lifecycle transition is uniquely identified by its aggregate version.
    // Repeated browser submits of that exact transition must therefore reach
    // the owner service with the same idempotency identity.
    format!(
        "identity:{principal_id}:{}:{expected_version}",
        action.capability()
    )
}

#[component]
pub fn IdentityAccessListSurface(page: IdentityAccessPage) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let heading = title(page.kind, locale);
    let slug = page.kind.slug();
    let result_count = format!(
        "{} {}",
        page.values.len(),
        copy(locale, "records", "รายการ")
    );
    let unavailable = page.evidence.availability == "unavailable";
    rsx! {
        article {
            class: "workspace-route identity-access-surface",
            "data-feature-owner": "identity-access",
            "data-record-kind": "{slug}",
            "data-freshness": "{page.evidence.freshness}",
            header {
                p { class: "eyebrow", {copy(locale, "IDENTITY & ACCESS", "ตัวตนและสิทธิ์เข้าถึง")} }
                h1 { "{heading}" }
                p { {copy(locale,
                    "Authoritative, scope-bound identity records with server-derived capabilities and explicit redaction.",
                    "รายการตัวตนจากแหล่งข้อมูลที่เชื่อถือได้ ผูกกับขอบเขต พร้อมความสามารถจากเซิร์ฟเวอร์และการปกปิดข้อมูลที่ชัดเจน",
                )} }
            }
            nav { class: "identity-record-tabs", aria_label: copy(locale, "Identity record types", "ประเภทข้อมูลตัวตน"),
                KitLink { href: "/identity/principal", {title(IdentityRecordKind::Principal, locale)} }
                KitLink { href: "/identity/role", {title(IdentityRecordKind::Role, locale)} }
                KitLink { href: "/identity/grant", {title(IdentityRecordKind::Grant, locale)} }
                KitLink { href: "/identity/access-review", {title(IdentityRecordKind::AccessReview, locale)} }
                KitLink { href: "/identity/break-glass-session", {title(IdentityRecordKind::BreakGlassSession, locale)} }
            }
            div { class: "route-meta", role: "status",
                span { "{page.evidence.availability} · {page.evidence.freshness}" }
                span { "{result_count}" }
                if let Some(measured_at) = &page.evidence.measured_at {
                    time { datetime: "{measured_at}", "{measured_at}" }
                }
            }
            if page.values.is_empty() {
                section { class: if unavailable { "empty-state empty-state--unavailable" } else { "empty-state" }, role: "status",
                    span { class: "empty-state__icon", aria_hidden: "true", if unavailable { "!" } else { "◇" } }
                    div { class: "empty-state__copy",
                        if unavailable {
                            h2 { {copy(locale, "Owner projection unavailable", "projection ของบริการเจ้าของยังไม่พร้อม")} }
                            p { {copy(locale,
                                "This record type is not connected to an authoritative projection. No empty or successful result has been inferred.",
                                "ประเภทรายการนี้ยังไม่เชื่อมกับ projection ที่เชื่อถือได้ ระบบจึงไม่อนุมานว่าผลลัพธ์ว่างหรือสำเร็จ",
                            )} }
                        } else {
                            h2 { {copy(locale, "No matching records", "ยังไม่มีรายการที่ตรงกัน")} }
                            p { {copy(locale,
                                "The authoritative projection returned no records for the active role and scope.",
                                "projection ที่เชื่อถือได้ไม่พบรายการสำหรับบทบาทและขอบเขตปัจจุบัน",
                            )} }
                        }
                    }
                }
            } else {
                ul { class: "record-list identity-record-list",
                    for record in page.values.iter() {
                        li { class: "record-row", key: "{record.id}",
                            KitLink { href: "/identity/{slug}/{record.id}",
                                span { class: "record-row__copy",
                                    strong { "{record.title}" }
                                    small { "{record.subtitle} · v{record.version}" }
                                }
                                span { class: "record-row__arrow", aria_hidden: "true", "→" }
                            }
                            span { class: "record-status", "{record.status}" }
                        }
                    }
                }
            }
            if let Some(cursor) = &page.next_cursor {
                nav { class: "route-actions", aria_label: copy(locale, "Pagination", "การแบ่งหน้า"),
                    KitLink { class: "route-action route-action--primary", href: "/identity/{slug}?cursor={cursor}",
                        {copy(locale, "Next page", "หน้าถัดไป")}
                    }
                }
            }
        }
    }
}

#[component]
pub fn IdentityAccessDetailSurface(
    detail: IdentityAccessDetail,
    on_principal_command: EventHandler<IdentityPrincipalCommandIntent>,
    command_feedback: Option<IdentityCommandFeedback>,
    command_busy: bool,
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let slug = detail.kind.slug();
    let mut purpose = use_signal(String::new);
    let mut mfa_code = use_signal(String::new);
    let mut confirmed = use_signal(|| false);
    let purpose_ready = !purpose().trim().is_empty();
    let mfa_ready = !mfa_code().trim().is_empty();
    let command_confirmed = confirmed();
    let actions = if detail.kind == IdentityRecordKind::Principal {
        available_principal_actions(&detail.record.status)
            .iter()
            .copied()
            .filter(|action| {
                detail
                    .record
                    .capabilities
                    .iter()
                    .any(|capability| capability == action.capability())
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    rsx! {
        article {
            class: "workspace-route identity-access-detail",
            "data-feature-owner": "identity-access",
            "data-record-kind": "{slug}",
            "data-freshness": "{detail.evidence.freshness}",
            nav { class: "route-breadcrumb", aria_label: copy(locale, "Breadcrumb", "เส้นทางนำทาง"),
                KitLink { href: "/identity/{slug}", {title(detail.kind, locale)} }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", "{detail.record.title}" }
            }
            header {
                p { class: "eyebrow", {copy(locale, "AUTHORIZED RECORD", "รายการตามสิทธิ์")} }
                h1 { "{detail.record.title}" }
                p { "{detail.record.subtitle}" }
            }
            div { class: "route-meta", role: "status",
                span { "{detail.record.status}" }
                span { "v{detail.record.version}" }
                span { "{detail.evidence.availability} · {detail.evidence.freshness}" }
            }
            dl { class: "record-detail__facts identity-detail__facts",
                div {
                    dt { {copy(locale, "Record ID", "รหัสรายการ")} }
                    dd { code { "{detail.record.id}" } }
                }
                for field in detail.record.fields.iter() {
                    div { class: if field.redacted { "record-field record-field--redacted" } else { "record-field" },
                        dt { "{field.label}" }
                        dd {
                            if field.redacted {
                                span { "••••" }
                                if let Some(reason) = &field.redaction_reason {
                                    small { "{reason}" }
                                }
                            } else {
                                "{field.value}"
                            }
                        }
                    }
                }
            }
            if !actions.is_empty() {
                section { class: "identity-command-panel", aria_label: copy(locale, "Principal lifecycle", "วงจรชีวิตตัวตนหลัก"),
                    h2 { {copy(locale, "Lifecycle command", "คำสั่งวงจรชีวิต")} }
                    p { {copy(locale,
                        "Commands require confirmation, a business purpose, and a fresh MFA proof. Success is shown only after the owner projection is current.",
                        "คำสั่งต้องมีการยืนยัน เหตุผลทางธุรกิจ และหลักฐาน MFA ล่าสุด ระบบจะแสดงว่าสำเร็จเมื่อ projection ของบริการเจ้าของเป็นปัจจุบันแล้วเท่านั้น",
                    )} }
                    label {
                        span { {copy(locale, "Purpose", "เหตุผล")} }
                        KitInput {
                            "data-testid": "identity-command-purpose",
                            value: "{purpose}",
                            maxlength: "256",
                            autocomplete: "off",
                            oninput: move |event: FormEvent| purpose.set(event.value()),
                        }
                    }
                    label {
                        span { {copy(locale, "Fresh MFA code", "รหัส MFA ล่าสุด")} }
                        KitInput {
                            "data-testid": "identity-command-mfa",
                            r#type: "password",
                            inputmode: "numeric",
                            maxlength: "512",
                            autocomplete: "one-time-code",
                            value: "{mfa_code}",
                            oninput: move |event: FormEvent| mfa_code.set(event.value()),
                        }
                    }
                    label { class: "identity-command-confirm",
                        KitInput {
                            "data-testid": "identity-command-confirm",
                            r#type: "checkbox",
                            checked: confirmed(),
                            onchange: move |event: FormEvent| confirmed.set(event.checked()),
                        }
                        span { {copy(locale, "I confirm this high-impact change", "ฉันยืนยันการเปลี่ยนแปลงที่มีผลกระทบสูงนี้")} }
                    }
                    div { class: "route-actions identity-command-actions",
                        for action in actions {
                            KitButton {
                                key: "{action.capability()}",
                                "data-testid": "identity-principal-command",
                                "data-action": "{action.capability()}",
                                "data-purpose-ready": purpose_ready,
                                "data-mfa-ready": mfa_ready,
                                "data-confirmed": command_confirmed,
                                class: if action == IdentityPrincipalLifecycleAction::Disable { "route-action route-action--danger" } else { "route-action route-action--primary" },
                                variant: if action == IdentityPrincipalLifecycleAction::Disable { ButtonVariant::Destructive } else { ButtonVariant::Primary },
                                r#type: "button",
                                disabled: command_busy || !command_confirmed || !purpose_ready || !mfa_ready,
                                onclick: {
                                    let principal_id = detail.record.id.clone();
                                    let purpose_value = purpose().clone();
                                    let mfa_value = mfa_code().clone();
                                    move |_| {
                                        on_principal_command.call(IdentityPrincipalCommandIntent {
                                            principal_id: principal_id.clone(),
                                            action,
                                            idempotency_key: principal_command_idempotency_key(
                                                &principal_id,
                                                action,
                                                detail.record.version,
                                            ),
                                            expected_version: detail.record.version,
                                            purpose: purpose_value.clone(),
                                            mfa_code: mfa_value.clone(),
                                        });
                                    }
                                },
                                if command_busy { "…" } else { {action_copy(action, locale)} }
                            }
                        }
                    }
                }
            }
            if let Some(feedback) = &command_feedback {
                div {
                    class: "identity-command-feedback",
                    role: "status",
                    "data-state": "{feedback.state}",
                    p { "{feedback.message}" }
                    if let Some(reference) = &feedback.audit_reference {
                        small { {copy(locale, "Audit reference", "หลักฐาน audit")}, ": ", code { "{reference}" } }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_kind_slugs_match_the_canonical_catalog() {
        assert_eq!(IdentityRecordKind::Principal.slug(), "principal");
        assert_eq!(IdentityRecordKind::Role.slug(), "role");
        assert_eq!(IdentityRecordKind::Grant.slug(), "grant");
        assert_eq!(IdentityRecordKind::AccessReview.slug(), "access-review");
        assert_eq!(
            IdentityRecordKind::BreakGlassSession.slug(),
            "break-glass-session"
        );
    }

    #[test]
    fn redaction_metadata_round_trips_without_exposing_a_value() {
        let field = IdentityField {
            label: "Directory subject".into(),
            value: String::new(),
            redacted: true,
            redaction_reason: Some("permission".into()),
        };
        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"redacted\":true"));
        assert!(!json.contains("external@example.com"));
    }

    #[test]
    fn lifecycle_actions_follow_principal_state_machine() {
        assert_eq!(
            available_principal_actions("invited"),
            &[
                IdentityPrincipalLifecycleAction::Activate,
                IdentityPrincipalLifecycleAction::Disable,
            ]
        );
        assert_eq!(
            available_principal_actions("active"),
            &[
                IdentityPrincipalLifecycleAction::Suspend,
                IdentityPrincipalLifecycleAction::Disable,
            ]
        );
        assert!(available_principal_actions("disabled").is_empty());
    }

    #[test]
    fn duplicate_lifecycle_submits_reuse_the_same_idempotency_identity() {
        let principal_id = uuid::Uuid::from_u128(1).to_string();
        let first = principal_command_idempotency_key(
            &principal_id,
            IdentityPrincipalLifecycleAction::Suspend,
            2,
        );
        assert_eq!(
            first,
            principal_command_idempotency_key(
                &principal_id,
                IdentityPrincipalLifecycleAction::Suspend,
                2,
            )
        );
        assert_ne!(
            first,
            principal_command_idempotency_key(
                &principal_id,
                IdentityPrincipalLifecycleAction::Suspend,
                3,
            )
        );
    }
}
