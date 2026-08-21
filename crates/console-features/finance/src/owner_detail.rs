//! Typed Phase 5 owner-detail surface for vendor invoices and payments.

use async_trait::async_trait;
use dioxus::prelude::*;
use erpx_integration_contracts::StableRecordRef;
use erpx_ui_core::Locale;
use erpx_ui_kit::{Button, ButtonTone, Link, Skeleton, StatusBadge, StatusTone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceSourceReference {
    pub record: StableRecordRef,
    pub version: u64,
    /// Owner-published label. Empty legacy labels remain unavailable rather
    /// than falling back to raw search text.
    pub display_label: String,
    pub canonical_route: String,
}

impl FinanceSourceReference {
    fn is_safe_for(&self, record_type: &str) -> bool {
        self.version > 0
            && !self.display_label.trim().is_empty()
            && self.record.record_type().as_str() == record_type
            && self.canonical_route == self.record.canonical_detail_route()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VendorInvoiceOwnerDetail {
    pub record: StableRecordRef,
    pub version: u64,
    pub invoice_code: String,
    pub supplier_label: String,
    pub currency: String,
    pub gross_minor: i64,
    pub outstanding_minor: i64,
    pub status: String,
    pub purchase_order: Option<FinanceSourceReference>,
    pub goods_receipts: Vec<FinanceSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentOwnerDetail {
    pub record: StableRecordRef,
    pub version: u64,
    pub payment_code: String,
    pub payee_label: String,
    pub currency: String,
    pub amount_minor: i64,
    pub status: String,
    pub invoices: Vec<FinanceSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinanceOwnerRecord {
    VendorInvoice(VendorInvoiceOwnerDetail),
    Payment(PaymentOwnerDetail),
}

impl FinanceOwnerRecord {
    pub fn record(&self) -> &StableRecordRef {
        match self {
            Self::VendorInvoice(value) => &value.record,
            Self::Payment(value) => &value.record,
        }
    }

    pub const fn version(&self) -> u64 {
        match self {
            Self::VendorInvoice(value) => value.version,
            Self::Payment(value) => value.version,
        }
    }

    fn is_safe_for(&self, expected_record: &StableRecordRef) -> bool {
        if self.record() != expected_record || self.version() == 0 {
            return false;
        }
        match self {
            Self::VendorInvoice(value) => {
                value
                    .purchase_order
                    .as_ref()
                    .is_none_or(|source| source.is_safe_for("procurement.purchase-order"))
                    && value
                        .goods_receipts
                        .iter()
                        .all(|source| source.is_safe_for("procurement.receipt"))
            }
            Self::Payment(value) => value
                .invoices
                .iter()
                .all(|source| source.is_safe_for("finance.vendor-invoice")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceOwnerCapabilities {
    pub post_vendor_invoice: bool,
    pub release_payment: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinanceOwnerDetailState {
    Loading,
    /// Current authorized data without a version-bound capability snapshot.
    /// This state is deliberately read-only.
    CurrentReadOnly {
        detail: FinanceOwnerRecord,
    },
    Fresh {
        detail: FinanceOwnerRecord,
        capabilities: FinanceOwnerCapabilities,
    },
    Stale {
        detail: FinanceOwnerRecord,
    },
    Denied,
    Offline,
    Unavailable {
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinanceOwnerMutation {
    PostVendorInvoice {
        record: StableRecordRef,
        expected_version: u64,
        purchase_order: StableRecordRef,
        purchase_order_version: u64,
        goods_receipt: StableRecordRef,
        goods_receipt_version: u64,
    },
    ReleasePayment {
        record: StableRecordRef,
        expected_version: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceOwnerMutationReceipt {
    pub record: StableRecordRef,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinanceOwnerGatewayError {
    Denied,
    Conflict,
    Stale,
    Offline,
    Unavailable,
    Invalid,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait FinanceOwnerGateway: Send + Sync {
    async fn load(
        &self,
        record: StableRecordRef,
    ) -> Result<FinanceOwnerDetailState, FinanceOwnerGatewayError>;

    async fn mutate(
        &self,
        mutation: FinanceOwnerMutation,
    ) -> Result<FinanceOwnerMutationReceipt, FinanceOwnerGatewayError>;
}

#[component]
pub fn FinanceOwnerDetailSurface(
    expected_record: StableRecordRef,
    state: FinanceOwnerDetailState,
    locale: Locale,
    #[props(default)] on_mutate: Option<EventHandler<FinanceOwnerMutation>>,
    #[props(default)] on_retry: Option<EventHandler<()>>,
    #[props(default)] mutation_busy: bool,
) -> Element {
    let invalid_payload = match &state {
        FinanceOwnerDetailState::CurrentReadOnly { detail }
        | FinanceOwnerDetailState::Fresh { detail, .. }
        | FinanceOwnerDetailState::Stale { detail } => !detail.is_safe_for(&expected_record),
        FinanceOwnerDetailState::Loading
        | FinanceOwnerDetailState::Denied
        | FinanceOwnerDetailState::Offline
        | FinanceOwnerDetailState::Unavailable { .. } => false,
    };
    if invalid_payload {
        return owner_notice(
            locale,
            "unavailable",
            "Finance owner data did not match this record. Actions are disabled.",
            "ข้อมูลจากเจ้าของรายการการเงินไม่ตรงกับรายการนี้ จึงปิดการดำเนินการ",
            None,
        );
    }

    match state {
        FinanceOwnerDetailState::Loading => rsx! {
            section { class: "owner-detail owner-detail--finance", Skeleton { lines: 5 } }
        },
        FinanceOwnerDetailState::CurrentReadOnly { detail } => rsx! {
            section { class: "owner-detail owner-detail--finance", "data-state": "current-read-only",
                p { role: "status", {copy(locale, "Owner data is current. Actions remain disabled until the owner publishes exact capabilities.", "ข้อมูลจากเจ้าของเป็นปัจจุบัน แต่ยังปิดการดำเนินการจนกว่าเจ้าของจะเผยแพร่ capability ที่ตรงกับรายการ")} }
                {render_detail(detail, None, locale, None, false)}
            }
        },
        FinanceOwnerDetailState::Fresh {
            detail,
            capabilities,
        } => render_detail(detail, Some(capabilities), locale, on_mutate, mutation_busy),
        FinanceOwnerDetailState::Stale { detail } => rsx! {
            section { class: "owner-detail owner-detail--finance", "data-state": "stale",
                p { role: "status", {copy(locale, "This owner projection is stale. Actions are disabled.", "โพรเจกชันจากเจ้าของล้าสมัย จึงปิดการดำเนินการ")} }
                {render_detail(detail, None, locale, None, false)}
            }
        },
        FinanceOwnerDetailState::Denied => owner_notice(
            locale,
            "denied",
            "You do not have access to this finance record.",
            "คุณไม่มีสิทธิ์เข้าถึงรายการการเงินนี้",
            None,
        ),
        FinanceOwnerDetailState::Offline => owner_notice(
            locale,
            "offline",
            "Finance owner data requires an online authorization check.",
            "ข้อมูลจากเจ้าของรายการการเงินต้องตรวจสอบสิทธิ์ขณะออนไลน์",
            None,
        ),
        FinanceOwnerDetailState::Unavailable { retryable } => owner_notice(
            locale,
            "unavailable",
            "Finance owner detail is temporarily unavailable.",
            "รายละเอียดจากเจ้าของรายการการเงินไม่พร้อมใช้งานชั่วคราว",
            retryable.then_some(on_retry).flatten(),
        ),
    }
}

/// Keeps the existing customer invoice route visibly separate from the
/// procure-to-pay vendor invoice owner contract.
#[component]
pub fn CustomerInvoiceArOwnerSlot(locale: Locale, owner_content: Element) -> Element {
    rsx! {
        section {
            class: "owner-detail owner-detail--customer-invoice",
            "data-record-kind": "customer-invoice-ar",
            p { class: "eyebrow", {copy(locale, "ACCOUNTS RECEIVABLE", "ลูกหนี้การค้า")} }
            h1 { {copy(locale, "Customer invoice", "ใบแจ้งหนี้ลูกค้า")} }
            p {
                {copy(
                    locale,
                    "This receivable is distinct from a supplier vendor invoice.",
                    "ลูกหนี้รายการนี้แยกจากใบแจ้งหนี้ผู้ขาย",
                )}
            }
            {owner_content}
        }
    }
}

fn render_detail(
    detail: FinanceOwnerRecord,
    capabilities: Option<FinanceOwnerCapabilities>,
    locale: Locale,
    on_mutate: Option<EventHandler<FinanceOwnerMutation>>,
    mutation_busy: bool,
) -> Element {
    let (eyebrow, code, status, currency, amount_minor, supporting, mutations) = match &detail {
        FinanceOwnerRecord::VendorInvoice(value) => {
            let mut mutations = Vec::new();
            if capabilities.is_some_and(|value| value.post_vendor_invoice)
                && let (Some(purchase_order), [goods_receipt]) =
                    (&value.purchase_order, value.goods_receipts.as_slice())
            {
                mutations.push((
                    copy(locale, "Post invoice", "บันทึกบัญชีใบแจ้งหนี้"),
                    FinanceOwnerMutation::PostVendorInvoice {
                        record: value.record.clone(),
                        expected_version: value.version,
                        purchase_order: purchase_order.record.clone(),
                        purchase_order_version: purchase_order.version,
                        goods_receipt: goods_receipt.record.clone(),
                        goods_receipt_version: goods_receipt.version,
                    },
                ));
            }
            (
                copy(locale, "VENDOR INVOICE", "ใบแจ้งหนี้ผู้ขาย"),
                value.invoice_code.clone(),
                value.status.clone(),
                value.currency.clone(),
                value.gross_minor,
                rsx! {
                    p { {value.supplier_label.clone()} }
                    p { {copy(locale, "Outstanding", "ยอดคงค้าง")} ": {value.outstanding_minor}" }
                    if let Some(source) = value.purchase_order.clone() {
                        Link { href: source.canonical_route, {source.display_label} }
                    }
                    for source in value.goods_receipts.clone() {
                        Link { href: source.canonical_route, {source.display_label} }
                    }
                },
                mutations,
            )
        }
        FinanceOwnerRecord::Payment(value) => {
            let mut mutations = Vec::new();
            if capabilities.is_some_and(|value| value.release_payment) {
                mutations.push((
                    copy(locale, "Release payment", "ปล่อยการชำระเงิน"),
                    FinanceOwnerMutation::ReleasePayment {
                        record: value.record.clone(),
                        expected_version: value.version,
                    },
                ));
            }
            (
                copy(locale, "PAYMENT", "การชำระเงิน"),
                value.payment_code.clone(),
                value.status.clone(),
                value.currency.clone(),
                value.amount_minor,
                rsx! {
                    p { {value.payee_label.clone()} }
                    if !value.invoices.is_empty() {
                        h3 { {copy(locale, "Authorized invoices", "ใบแจ้งหนี้ที่ได้รับอนุญาต")} }
                        for source in value.invoices.clone() {
                            Link { href: source.canonical_route, {source.display_label} }
                        }
                    }
                },
                mutations,
            )
        }
    };
    rsx! {
        section { class: "owner-detail owner-detail--finance", "data-state": if capabilities.is_some() { "fresh" } else { "read-only" },
            p { class: "eyebrow", {eyebrow} }
            h1 { {code} }
            StatusBadge { label: status, tone: StatusTone::Neutral }
            p { class: "owner-detail__amount", "{currency} {amount_minor}" }
            {supporting}
            if let Some(handler) = on_mutate {
                for (label, mutation) in mutations {
                    Button {
                        label: label.to_owned(),
                        tone: ButtonTone::Primary,
                        disabled: mutation_busy,
                        busy: mutation_busy,
                        on_press: move |_| handler.call(mutation.clone()),
                    }
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
        section { class: "owner-detail owner-detail--finance", "data-state": state, role: "status",
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
    fn post_and_release_require_fresh_capabilities() {
        let source = include_str!("owner_detail.rs");
        let fresh = source.find("FinanceOwnerDetailState::Fresh").unwrap();
        let read_only = source
            .find("FinanceOwnerDetailState::CurrentReadOnly")
            .unwrap();
        let stale = source.find("FinanceOwnerDetailState::Stale").unwrap();
        let render = source.find("fn render_detail").unwrap();
        assert!(read_only < fresh && fresh < stale && stale < render);
        assert!(source.contains("capabilities.is_some_and"));
        assert!(source.contains("expected_version"));
        assert!(source.contains("!detail.is_safe_for(&expected_record)"));
        assert!(source.contains("self.canonical_route == self.record.canonical_detail_route()"));
        assert!(source.contains("data-record-kind\": \"customer-invoice-ar"));
        assert!(!source.contains(concat!("ApproveVendor", "Invoice")));
        assert!(!source.contains(concat!("Approve", "Payment")));
        assert!(!source.contains(concat!("erpx_", "proto")));
        assert!(!source.contains(concat!("ton", "ic::")));
    }
}
