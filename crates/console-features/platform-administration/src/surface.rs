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
) -> Element {
    let locale = erpx_ui_core::use_locale().current();
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
}
