use dioxus::prelude::*;
use erpx_ui_core::Locale;
use erpx_ui_kit::kit::Link as KitLink;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldRedaction {
    pub field: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingProjectionEvidence {
    pub availability: String,
    pub freshness: String,
    pub measured_at: Option<String>,
    pub cursor: Option<String>,
    pub correlation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingDetail {
    pub id: String,
    pub customer_name: String,
    pub lifecycle: String,
    pub registry_version: u64,
    pub routing: String,
    pub projection: String,
    pub backup_pitr: String,
    pub storage: String,
    pub configuration: String,
    pub subscription: String,
    pub integration_health: String,
    pub health: String,
    pub projected_at: Option<String>,
    pub enterable: bool,
    pub capabilities: Vec<String>,
    pub redactions: Vec<FieldRedaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingPage {
    pub values: Vec<HoldingDetail>,
    pub next_cursor: Option<String>,
    pub page_size: u16,
    pub evidence: HoldingProjectionEvidence,
    pub capabilities: Vec<String>,
    pub redactions: Vec<FieldRedaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingControlHistoryEntry {
    pub event_id: String,
    pub event_category: String,
    pub safe_action: String,
    pub aggregate_version: u64,
    pub actor_ref: String,
    pub correlation_id: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HoldingControlHistoryPage {
    pub values: Vec<HoldingControlHistoryEntry>,
    pub next_cursor: Option<String>,
    pub page_size: u16,
    pub availability: String,
    pub freshness: String,
    pub measured_at: Option<String>,
    pub correlation_id: String,
    pub capabilities: Vec<String>,
    pub redactions: Vec<FieldRedaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum HoldingControlHistoryState {
    Ready {
        page: HoldingControlHistoryPage,
    },
    Forbidden {
        reason: String,
        correlation_id: String,
    },
    Unavailable {
        reason: String,
        correlation_id: String,
    },
}

fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

fn optional_fact(value: &str) -> &str {
    if value.trim().is_empty() {
        "—"
    } else {
        value
    }
}

#[component]
pub fn HoldingsSurface(page: HoldingPage) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let result_count = format!(
        "{} {}",
        page.values.len(),
        copy(locale, "holdings", "โฮลดิ้ง")
    );
    let freshness = format!(
        "{} · {}",
        page.evidence.availability, page.evidence.freshness
    );
    rsx! {
        article {
            class: "workspace-route platform-holdings-surface",
            "data-feature-owner": "platform-administration",
            "data-freshness": "{page.evidence.freshness}",
            header {
                p { class: "eyebrow", {copy(locale, "PLATFORM ADMINISTRATION", "การดูแลแพลตฟอร์ม")} }
                h1 { {copy(locale, "Platform holdings", "โฮลดิ้งบนแพลตฟอร์ม")} }
                p { {copy(locale,
                    "Authoritative holding lifecycle, routing, projection, storage, and recovery evidence.",
                    "ข้อมูลวงจรชีวิต เส้นทางข้อมูล projection พื้นที่จัดเก็บ และการกู้คืนจากแหล่งข้อมูลที่เชื่อถือได้",
                )} }
            }
            div { class: "route-meta", role: "status",
                span { "{freshness}" }
                span { "{result_count}" }
                if let Some(measured_at) = &page.evidence.measured_at {
                    time { datetime: "{measured_at}", "{measured_at}" }
                }
            }
            if page.values.is_empty() {
                section { class: "empty-state", role: "status",
                    span { class: "empty-state__icon", aria_hidden: "true", "◇" }
                    div { class: "empty-state__copy",
                        h2 { {copy(locale, "No registered holdings", "ยังไม่มีโฮลดิ้งที่ลงทะเบียน")} }
                        p { {copy(locale,
                            "The authoritative platform projection returned an empty page.",
                            "projection ที่เชื่อถือได้ของแพลตฟอร์มส่งคืนหน้าว่าง",
                        )} }
                    }
                }
            } else {
                ul { class: "record-list holding-record-list",
                    for holding in page.values.iter() {
                        li { class: "record-row holding-record", key: "{holding.id}",
                            KitLink { href: "/platform/holdings/{holding.id}",
                                span { class: "record-row__copy",
                                    strong { "{holding.customer_name}" }
                                    small { "{holding.id} · v{holding.registry_version}" }
                                }
                                span { class: "holding-record__signals",
                                    small { "{holding.projection}" }
                                    small { "{holding.backup_pitr}" }
                                }
                                span { class: "record-row__arrow", aria_hidden: "true", "→" }
                            }
                            span { class: "record-status", "{holding.lifecycle}" }
                        }
                    }
                }
            }
            if let Some(cursor) = &page.next_cursor {
                nav { class: "route-actions", aria_label: copy(locale, "Pagination", "การแบ่งหน้า"),
                    KitLink { class: "route-action route-action--primary", href: "/platform/holdings?cursor={cursor}",
                        {copy(locale, "Next page", "หน้าถัดไป")}
                    }
                }
            }
        }
    }
}

#[component]
pub fn PlatformHoldingDetailSurface(
    holding: HoldingDetail,
    evidence: HoldingProjectionEvidence,
    history: HoldingControlHistoryState,
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
    let history_event_count = match &history {
        HoldingControlHistoryState::Ready { page } => format!(
            "{} {}",
            page.values.len(),
            copy(locale, "events", "เหตุการณ์")
        ),
        HoldingControlHistoryState::Forbidden { .. }
        | HoldingControlHistoryState::Unavailable { .. } => String::new(),
    };
    rsx! {
        article {
            class: "workspace-route platform-holding-detail",
            "data-feature-owner": "platform-administration",
            "data-freshness": "{evidence.freshness}",
            nav { class: "route-breadcrumb", aria_label: copy(locale, "Breadcrumb", "เส้นทางนำทาง"),
                KitLink { href: "/platform/holdings", {copy(locale, "Platform holdings", "โฮลดิ้งบนแพลตฟอร์ม")} }
                span { aria_hidden: "true", "/" }
                span { aria_current: "page", "{holding.customer_name}" }
            }
            header {
                p { class: "eyebrow", {copy(locale, "AUTHORITATIVE HOLDING", "โฮลดิ้งที่เชื่อถือได้")} }
                h1 { "{holding.customer_name}" }
                p { "{holding.id}" }
            }
            div { class: "route-meta", role: "status",
                span { "{holding.lifecycle}" }
                span { "v{holding.registry_version}" }
                span { "{evidence.availability} · {evidence.freshness}" }
            }
            dl { class: "record-detail__facts holding-detail__facts",
                div { dt { {copy(locale, "Routing", "เส้นทางข้อมูล")} } dd { "{optional_fact(&holding.routing)}" } }
                div { dt { "Projection" } dd { "{optional_fact(&holding.projection)}" } }
                div { dt { "Backup / PITR" } dd { "{optional_fact(&holding.backup_pitr)}" } }
                div { dt { {copy(locale, "Storage", "พื้นที่จัดเก็บ")} } dd { "{optional_fact(&holding.storage)}" } }
                div { dt { {copy(locale, "Configuration", "การกำหนดค่า")} } dd { "{optional_fact(&holding.configuration)}" } }
                div { dt { {copy(locale, "Subscription", "การสมัครใช้งาน")} } dd { "{optional_fact(&holding.subscription)}" } }
                div { dt { {copy(locale, "Integrations", "การเชื่อมต่อ")} } dd { "{optional_fact(&holding.integration_health)}" } }
                div { dt { {copy(locale, "Overall health", "สุขภาพโดยรวม")} } dd { "{optional_fact(&holding.health)}" } }
            }
            if !holding.redactions.is_empty() {
                section { class: "redaction-notice", role: "note",
                    h2 { {copy(locale, "Protected fields", "ข้อมูลที่ได้รับการปกป้อง")} }
                    ul {
                        for redaction in holding.redactions.iter() {
                            li { code { "{redaction.field}" } " — {redaction.reason}" }
                        }
                    }
                }
            }
            section { class: "holding-history", aria_label: copy(locale, "Holding audit history", "ประวัติการตรวจสอบโฮลดิ้ง"),
                h2 { {copy(locale, "Audit history", "ประวัติการตรวจสอบ")} }
                p { {copy(locale,
                    "Safe lifecycle evidence from the authoritative platform control history. Internal payloads and persistence keys are never returned.",
                    "หลักฐานวงจรชีวิตที่ปลอดภัยจากประวัติการควบคุมแพลตฟอร์มที่เชื่อถือได้ โดยไม่ส่งคืน payload ภายในหรือคีย์การจัดเก็บ",
                )} }
                match &history {
                    HoldingControlHistoryState::Ready { page } => rsx! {
                        div { class: "route-meta", role: "status",
                            span { "{page.availability} · {page.freshness}" }
                            span { "{history_event_count}" }
                            if let Some(measured_at) = &page.measured_at {
                                time { datetime: "{measured_at}", "{measured_at}" }
                            }
                        }
                        if page.values.is_empty() {
                            div { class: "empty-state", role: "status",
                                span { class: "empty-state__icon", aria_hidden: "true", "◇" }
                                div { class: "empty-state__copy",
                                    h3 { {copy(locale, "No control events", "ยังไม่มีเหตุการณ์ควบคุม")} }
                                    p { {copy(locale,
                                        "The authoritative history returned an empty page.",
                                        "ประวัติที่เชื่อถือได้ส่งคืนหน้าว่าง",
                                    )} }
                                }
                            }
                        } else {
                            ol { class: "holding-history__list",
                                for entry in page.values.iter() {
                                    li { key: "{entry.event_id}", class: "holding-history__event",
                                        div {
                                            strong { "{entry.safe_action}" }
                                            small { "{entry.event_category} · v{entry.aggregate_version}" }
                                        }
                                        dl {
                                            div { dt { {copy(locale, "Occurred", "เกิดเมื่อ")} } dd { time { datetime: "{entry.occurred_at}", "{entry.occurred_at}" } } }
                                            div { dt { {copy(locale, "Actor", "ผู้ดำเนินการ")} } dd { code { "{entry.actor_ref}" } } }
                                            div { dt { {copy(locale, "Correlation", "รหัสเชื่อมโยง")} } dd { code { "{entry.correlation_id}" } } }
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(cursor) = &page.next_cursor {
                            nav { class: "route-actions", aria_label: copy(locale, "History pagination", "การแบ่งหน้าประวัติ"),
                                KitLink { class: "route-action route-action--primary", href: "/platform/holdings/{holding.id}?history_cursor={cursor}",
                                    {copy(locale, "Older events", "เหตุการณ์ก่อนหน้า")}
                                }
                            }
                        }
                        if !page.redactions.is_empty() {
                            div { class: "redaction-notice", role: "note",
                                strong { {copy(locale, "History redactions", "การปกปิดข้อมูลประวัติ")} }
                                ul {
                                    for redaction in page.redactions.iter() {
                                        li { code { "{redaction.field}" } " — {redaction.reason}" }
                                    }
                                }
                            }
                        }
                    },
                    HoldingControlHistoryState::Forbidden { reason, correlation_id } => rsx! {
                        div { class: "empty-state", role: "status",
                            span { class: "empty-state__icon", aria_hidden: "true", "⊘" }
                            div { class: "empty-state__copy",
                                h3 { {copy(locale, "History not authorized", "ไม่มีสิทธิ์ดูประวัติ")} }
                                p { "{reason}" }
                                small { {copy(locale, "Correlation", "รหัสเชื่อมโยง")} ": " code { "{correlation_id}" } }
                            }
                        }
                    },
                    HoldingControlHistoryState::Unavailable { reason, correlation_id } => rsx! {
                        div { class: "empty-state empty-state--unavailable", role: "status",
                            span { class: "empty-state__icon", aria_hidden: "true", "!" }
                            div { class: "empty-state__copy",
                                h3 { {copy(locale, "History unavailable", "ประวัติไม่พร้อมใช้งาน")} }
                                p { "{reason}" }
                                small { {copy(locale, "Correlation", "รหัสเชื่อมโยง")} ": " code { "{correlation_id}" } }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holding_dto_retains_version_freshness_and_redaction_evidence() {
        let json = serde_json::to_value(HoldingPage {
            values: vec![HoldingDetail {
                id: "holding-1".into(),
                customer_name: "Example".into(),
                lifecycle: "active".into(),
                registry_version: 7,
                routing: "ready".into(),
                projection: "current".into(),
                backup_pitr: "verified".into(),
                storage: "ready".into(),
                configuration: "ready".into(),
                subscription: "active".into(),
                integration_health: "healthy".into(),
                health: "healthy".into(),
                projected_at: Some("2026-08-20T00:00:00Z".into()),
                enterable: true,
                capabilities: vec!["GetHolding".into()],
                redactions: vec![FieldRedaction {
                    field: "customer_id".into(),
                    reason: "permission".into(),
                }],
            }],
            next_cursor: None,
            page_size: 25,
            evidence: HoldingProjectionEvidence {
                availability: "available".into(),
                freshness: "current".into(),
                measured_at: Some("2026-08-20T00:00:00Z".into()),
                cursor: None,
                correlation_id: "correlation".into(),
            },
            capabilities: vec!["ListHoldings".into()],
            redactions: Vec::new(),
        })
        .unwrap();
        assert_eq!(json["values"][0]["registry_version"], 7);
        assert_eq!(json["evidence"]["freshness"], "current");
        assert_eq!(json["values"][0]["redactions"][0]["field"], "customer_id");
    }

    #[test]
    fn history_state_keeps_cursor_version_and_safe_reason_explicit() {
        let ready = HoldingControlHistoryState::Ready {
            page: HoldingControlHistoryPage {
                values: vec![HoldingControlHistoryEntry {
                    event_id: "00000000-0000-0000-0000-000000000001".into(),
                    event_category: "lifecycle".into(),
                    safe_action: "holding suspended".into(),
                    aggregate_version: 4,
                    actor_ref: "principal:operator".into(),
                    correlation_id: "00000000-0000-0000-0000-000000000002".into(),
                    occurred_at: "2026-08-21T00:00:00Z".into(),
                }],
                next_cursor: Some("1.00000000-0000-0000-0000-000000000001".into()),
                page_size: 25,
                availability: "available".into(),
                freshness: "current".into(),
                measured_at: Some("2026-08-21T00:00:00Z".into()),
                correlation_id: "00000000-0000-0000-0000-000000000002".into(),
                capabilities: vec!["ListHoldingControlHistory".into()],
                redactions: vec![FieldRedaction {
                    field: "event_payload".into(),
                    reason: "not exposed".into(),
                }],
            },
        };
        let json = serde_json::to_value(ready).unwrap();
        assert_eq!(json["page"]["values"][0]["aggregate_version"], 4);
        assert_eq!(json["page"]["redactions"][0]["field"], "event_payload");

        let unavailable = serde_json::to_value(HoldingControlHistoryState::Unavailable {
            reason: "holding_control_history_unavailable".into(),
            correlation_id: "00000000-0000-0000-0000-000000000003".into(),
        })
        .unwrap();
        assert_eq!(unavailable["reason"], "holding_control_history_unavailable");
    }
}
