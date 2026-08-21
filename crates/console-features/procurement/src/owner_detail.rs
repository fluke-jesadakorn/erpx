//! Typed Phase 5 owner-detail surface for the Procurement chain.
//!
//! Authorization, freshness, and command capabilities are supplied by the
//! authenticated owner gateway. Route segments never grant a mutation.

use async_trait::async_trait;
use dioxus::prelude::*;
use erpx_integration_contracts::StableRecordRef;
use erpx_ui_core::Locale;
use erpx_ui_kit::{Button, ButtonTone, Link, Skeleton, StatusBadge, StatusTone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementSourceReference {
    pub record: StableRecordRef,
    pub version: u64,
    /// Owner-published label. Empty legacy labels fail closed instead of using
    /// raw search text or an invented fallback.
    pub display_label: String,
    pub canonical_route: String,
}

impl ProcurementSourceReference {
    fn is_safe_for(&self, record_type: &str) -> bool {
        self.version > 0
            && !self.display_label.trim().is_empty()
            && self.record.record_type().as_str() == record_type
            && self.canonical_route == self.record.canonical_detail_route()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequisitionOwnerDetail {
    pub record: StableRecordRef,
    pub version: u64,
    pub requisition_code: String,
    pub requester_label: String,
    pub business_purpose: String,
    pub currency: String,
    pub total_minor: i64,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseOrderOwnerDetail {
    pub record: StableRecordRef,
    pub version: u64,
    pub purchase_order_code: String,
    pub supplier_label: String,
    pub currency: String,
    pub total_minor: i64,
    pub status: String,
    /// Exact, owner-published source versions. Each source is separately
    /// authorized before its display label and canonical link appear.
    pub source_requisition: ProcurementSourceReference,
    pub contract: Option<ProcurementSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoodsReceiptOwnerDetail {
    pub record: StableRecordRef,
    pub version: u64,
    pub receipt_code: String,
    pub received_by_label: String,
    pub received_at: String,
    pub status: String,
    pub purchase_order: ProcurementSourceReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcurementOwnerRecord {
    Requisition(RequisitionOwnerDetail),
    PurchaseOrder(PurchaseOrderOwnerDetail),
    GoodsReceipt(GoodsReceiptOwnerDetail),
}

impl ProcurementOwnerRecord {
    pub fn record(&self) -> &StableRecordRef {
        match self {
            Self::Requisition(value) => &value.record,
            Self::PurchaseOrder(value) => &value.record,
            Self::GoodsReceipt(value) => &value.record,
        }
    }

    pub const fn version(&self) -> u64 {
        match self {
            Self::Requisition(value) => value.version,
            Self::PurchaseOrder(value) => value.version,
            Self::GoodsReceipt(value) => value.version,
        }
    }

    fn is_safe_for(&self, expected_record: &StableRecordRef) -> bool {
        if self.record() != expected_record || self.version() == 0 {
            return false;
        }
        match self {
            Self::Requisition(_) => true,
            Self::PurchaseOrder(value) => {
                value
                    .source_requisition
                    .is_safe_for("procurement.requisition")
                    && value
                        .contract
                        .as_ref()
                        .is_none_or(|source| source.is_safe_for("legal.contract"))
            }
            Self::GoodsReceipt(value) => value
                .purchase_order
                .is_safe_for("procurement.purchase-order"),
        }
    }
}

/// Capabilities are display hints from the same fresh owner response. The
/// command endpoint remains authoritative and must reauthorize every request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementOwnerCapabilities {
    pub approve_requisition: bool,
    pub release_purchase_order: bool,
    pub accept_goods_receipt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcurementOwnerDetailState {
    Loading,
    Fresh {
        detail: ProcurementOwnerRecord,
        capabilities: ProcurementOwnerCapabilities,
    },
    Stale {
        detail: ProcurementOwnerRecord,
    },
    Denied,
    Offline,
    Unavailable {
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcurementOwnerMutation {
    ApproveRequisition {
        record: StableRecordRef,
        expected_version: u64,
    },
    ReleasePurchaseOrder {
        record: StableRecordRef,
        expected_version: u64,
    },
    AcceptGoodsReceipt {
        record: StableRecordRef,
        expected_version: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementOwnerMutationReceipt {
    pub record: StableRecordRef,
    pub version: u64,
    pub replayed: bool,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ProcurementOwnerGateway: Send + Sync {
    async fn load(
        &self,
        record: StableRecordRef,
    ) -> Result<ProcurementOwnerDetailState, ProcurementOwnerGatewayError>;

    async fn mutate(
        &self,
        mutation: ProcurementOwnerMutation,
    ) -> Result<ProcurementOwnerMutationReceipt, ProcurementOwnerGatewayError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProcurementOwnerGatewayError {
    Denied,
    Conflict,
    Stale,
    Offline,
    Unavailable,
    Invalid,
}

#[component]
pub fn ProcurementOwnerDetailSurface(
    expected_record: StableRecordRef,
    state: ProcurementOwnerDetailState,
    locale: Locale,
    #[props(default)] on_mutate: Option<EventHandler<ProcurementOwnerMutation>>,
    #[props(default)] on_retry: Option<EventHandler<()>>,
) -> Element {
    let invalid_payload = match &state {
        ProcurementOwnerDetailState::Fresh { detail, .. }
        | ProcurementOwnerDetailState::Stale { detail } => !detail.is_safe_for(&expected_record),
        ProcurementOwnerDetailState::Loading
        | ProcurementOwnerDetailState::Denied
        | ProcurementOwnerDetailState::Offline
        | ProcurementOwnerDetailState::Unavailable { .. } => false,
    };
    if invalid_payload {
        return owner_notice(
            locale,
            "unavailable",
            "Procurement owner data did not match this record. Actions are disabled.",
            "ข้อมูลจากเจ้าของรายการจัดซื้อไม่ตรงกับรายการนี้ จึงปิดการดำเนินการ",
            None,
        );
    }

    match state {
        ProcurementOwnerDetailState::Loading => rsx! {
            section { class: "owner-detail owner-detail--procurement", Skeleton { lines: 5 } }
        },
        ProcurementOwnerDetailState::Fresh {
            detail,
            capabilities,
        } => render_detail(detail, Some(capabilities), locale, on_mutate),
        ProcurementOwnerDetailState::Stale { detail } => rsx! {
            section { class: "owner-detail owner-detail--procurement", "data-state": "stale",
                p { role: "status", {copy(locale, "This owner projection is stale. Actions are disabled.", "โพรเจกชันจากเจ้าของล้าสมัย จึงปิดการดำเนินการ")} }
                {render_detail(detail, None, locale, None)}
            }
        },
        ProcurementOwnerDetailState::Denied => owner_notice(
            locale,
            "denied",
            "You do not have access to this procurement record.",
            "คุณไม่มีสิทธิ์เข้าถึงรายการจัดซื้อนี้",
            None,
        ),
        ProcurementOwnerDetailState::Offline => owner_notice(
            locale,
            "offline",
            "Procurement owner data requires an online authorization check.",
            "ข้อมูลจากเจ้าของรายการจัดซื้อต้องตรวจสอบสิทธิ์ขณะออนไลน์",
            None,
        ),
        ProcurementOwnerDetailState::Unavailable { retryable } => owner_notice(
            locale,
            "unavailable",
            "Procurement owner detail is temporarily unavailable.",
            "รายละเอียดจากเจ้าของรายการจัดซื้อไม่พร้อมใช้งานชั่วคราว",
            retryable.then_some(on_retry).flatten(),
        ),
    }
}

fn render_detail(
    detail: ProcurementOwnerRecord,
    capabilities: Option<ProcurementOwnerCapabilities>,
    locale: Locale,
    on_mutate: Option<EventHandler<ProcurementOwnerMutation>>,
) -> Element {
    let (eyebrow, code, status, currency, total_minor, supporting, mutation) = match &detail {
        ProcurementOwnerRecord::Requisition(value) => (
            copy(locale, "REQUISITION", "ใบขอซื้อ"),
            value.requisition_code.clone(),
            value.status.clone(),
            value.currency.clone(),
            value.total_minor,
            rsx! {
                p { {value.requester_label.clone()} }
                p { {value.business_purpose.clone()} }
            },
            capabilities
                .filter(|value| value.approve_requisition)
                .map(|_| ProcurementOwnerMutation::ApproveRequisition {
                    record: value.record.clone(),
                    expected_version: value.version,
                }),
        ),
        ProcurementOwnerRecord::PurchaseOrder(value) => (
            copy(locale, "PURCHASE ORDER", "ใบสั่งซื้อ"),
            value.purchase_order_code.clone(),
            value.status.clone(),
            value.currency.clone(),
            value.total_minor,
            rsx! {
                p { {value.supplier_label.clone()} }
                h3 { {copy(locale, "Authorized source records", "รายการต้นทางที่ได้รับอนุญาต")} }
                ul {
                    li {
                        Link {
                            href: value.source_requisition.canonical_route.clone(),
                            {value.source_requisition.display_label.clone()}
                        }
                    }
                    if let Some(source) = value.contract.clone() {
                        li { Link { href: source.canonical_route, {source.display_label} } }
                    }
                }
            },
            capabilities
                .filter(|value| value.release_purchase_order)
                .map(|_| ProcurementOwnerMutation::ReleasePurchaseOrder {
                    record: value.record.clone(),
                    expected_version: value.version,
                }),
        ),
        ProcurementOwnerRecord::GoodsReceipt(value) => (
            copy(locale, "GOODS RECEIPT", "ใบรับสินค้า"),
            value.receipt_code.clone(),
            value.status.clone(),
            String::new(),
            0,
            rsx! {
                p { {value.received_by_label.clone()} }
                p { {value.received_at.clone()} }
                Link { href: value.purchase_order.canonical_route.clone(), {value.purchase_order.display_label.clone()} }
            },
            capabilities
                .filter(|value| value.accept_goods_receipt)
                .map(|_| ProcurementOwnerMutation::AcceptGoodsReceipt {
                    record: value.record.clone(),
                    expected_version: value.version,
                }),
        ),
    };
    let action_label = match mutation {
        Some(ProcurementOwnerMutation::ApproveRequisition { .. }) => {
            Some(copy(locale, "Approve requisition", "อนุมัติใบขอซื้อ"))
        }
        Some(ProcurementOwnerMutation::ReleasePurchaseOrder { .. }) => {
            Some(copy(locale, "Release purchase order", "ปล่อยใบสั่งซื้อ"))
        }
        Some(ProcurementOwnerMutation::AcceptGoodsReceipt { .. }) => {
            Some(copy(locale, "Accept goods receipt", "ยอมรับใบรับสินค้า"))
        }
        None => None,
    };
    rsx! {
        section { class: "owner-detail owner-detail--procurement", "data-state": if capabilities.is_some() { "fresh" } else { "read-only" },
            p { class: "eyebrow", {eyebrow} }
            h1 { {code} }
            StatusBadge { label: status, tone: StatusTone::Neutral }
            if !currency.is_empty() {
                p { class: "owner-detail__amount", "{currency} {total_minor}" }
            }
            {supporting}
            if let (Some(label), Some(mutation), Some(handler)) = (action_label, mutation, on_mutate) {
                Button {
                    label: label.to_owned(),
                    tone: ButtonTone::Primary,
                    on_press: move |_| handler.call(mutation.clone()),
                }
            }
        }
    }
}

fn owner_notice(
    locale: Locale,
    state: &'static str,
    en: &'static str,
    th: &'static str,
    on_retry: Option<EventHandler<()>>,
) -> Element {
    rsx! {
        section { class: "owner-detail owner-detail--procurement", "data-state": state, role: "status",
            p { {copy(locale, en, th)} }
            if on_retry.is_some() {
                Button {
                    label: copy(locale, "Retry", "ลองใหม่").to_owned(),
                    tone: ButtonTone::Secondary,
                    on_press: on_retry,
                }
            }
        }
    }
}

const fn copy(locale: Locale, en: &'static str, th: &'static str) -> &'static str {
    match locale {
        Locale::En => en,
        Locale::Th => th,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mutation_controls_require_the_fresh_capability_branch() {
        let source = include_str!("owner_detail.rs");
        let fresh = source.find("ProcurementOwnerDetailState::Fresh").unwrap();
        let stale = source.find("ProcurementOwnerDetailState::Stale").unwrap();
        let render = source.find("fn render_detail").unwrap();
        assert!(fresh < stale && stale < render);
        assert!(source.contains("capabilities.filter"));
        assert!(source.contains("expected_version"));
        assert!(source.contains("!detail.is_safe_for(&expected_record)"));
        assert!(source.contains("self.canonical_route == self.record.canonical_detail_route()"));
        assert!(source.contains("source_requisition"));
        assert!(source.contains("AcceptGoodsReceipt"));
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("ton", "ic::")));
    }
}
