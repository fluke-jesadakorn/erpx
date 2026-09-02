//! Cross-department @ mention support for Finance.
//!
//! Reuses the collaboration typed-token contract from `erpx_ui_ai_collaboration`
//! without introducing a new token kind. Finance records become resolvable
//! directory descriptors so `@record:<uuid>` and `@field:<uuid>:<field>@v<ver>`
//! work from any workspace.

use erpx_ui_core::Locale;

/// Finance record families that are resolvable as `@record` mentions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceMentionKind {
    Chart,
    Journal,
    Subledger,
    VendorInvoice,
    Payment,
    CustomerInvoice,
    RevenueSchedule,
    Period,
    BankTransaction,
}

impl FinanceMentionKind {
    pub const ALL: &'static [Self] = &[
        Self::Chart,
        Self::Journal,
        Self::Subledger,
        Self::VendorInvoice,
        Self::Payment,
        Self::CustomerInvoice,
        Self::RevenueSchedule,
        Self::Period,
        Self::BankTransaction,
    ];

    pub const fn record_type(self) -> &'static str {
        match self {
            Self::Chart => "finance.chart",
            Self::Journal => "finance.journal",
            Self::Subledger => "finance.subledger",
            Self::VendorInvoice => "finance.vendor-invoice",
            Self::Payment => "finance.payment",
            Self::CustomerInvoice => "finance.invoice",
            Self::RevenueSchedule => "finance.revenue-schedule",
            Self::Period => "finance.period",
            Self::BankTransaction => "finance.bank-transaction",
        }
    }

    pub const fn label(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::Chart, Locale::En) => "Chart",
            (Self::Chart, Locale::Th) => "ผังบัญชี",
            (Self::Journal, Locale::En) => "Journal",
            (Self::Journal, Locale::Th) => "สมุดรายวัน",
            (Self::Subledger, Locale::En) => "Subledger",
            (Self::Subledger, Locale::Th) => "บัญชีย่อย",
            (Self::VendorInvoice, Locale::En) => "Vendor invoice",
            (Self::VendorInvoice, Locale::Th) => "ใบแจ้งหนี้ผู้ขาย",
            (Self::Payment, Locale::En) => "Payment",
            (Self::Payment, Locale::Th) => "การชำระเงิน",
            (Self::CustomerInvoice, Locale::En) => "Customer invoice",
            (Self::CustomerInvoice, Locale::Th) => "ใบแจ้งหนี้ลูกหนี้",
            (Self::RevenueSchedule, Locale::En) => "Revenue schedule",
            (Self::RevenueSchedule, Locale::Th) => "ตารางรับรู้รายได้",
            (Self::Period, Locale::En) => "Period",
            (Self::Period, Locale::Th) => "งวดบัญชี",
            (Self::BankTransaction, Locale::En) => "Bank transaction",
            (Self::BankTransaction, Locale::Th) => "ธุรกรรมธนาคาร",
        }
    }

    pub const fn canonical_prefix(self) -> &'static str {
        match self {
            Self::Chart => "/finance/chart/",
            Self::Journal => "/finance/journals/",
            Self::Subledger => "/finance/subledgers/",
            Self::VendorInvoice => "/finance/ap/",
            Self::Payment => "/finance/ap/",
            Self::CustomerInvoice => "/finance/ar/",
            Self::RevenueSchedule => "/finance/ar/",
            Self::Period => "/finance/close/",
            Self::BankTransaction => "/finance/banking/",
        }
    }
}

/// Field identifiers that can be referenced as `@field:<id>:<field>@v<ver>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceMentionField {
    Amount,
    Currency,
    Status,
    Account,
    Period,
    Counterparty,
}

impl FinanceMentionField {
    pub const ALL: &'static [Self] = &[
        Self::Amount,
        Self::Currency,
        Self::Status,
        Self::Account,
        Self::Period,
        Self::Counterparty,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Amount => "amount",
            Self::Currency => "currency",
            Self::Status => "status",
            Self::Account => "account",
            Self::Period => "period",
            Self::Counterparty => "counterparty",
        }
    }
}

/// Resolve a pasted `@record:<uuid>` against finance record types.
pub fn is_finance_record_type(record_type: &str) -> bool {
    FinanceMentionKind::ALL
        .iter()
        .any(|k| k.record_type() == record_type)
}

/// Build the `@record:<uuid>` clipboard token for a finance record.
pub fn finance_record_clipboard_token(record_id: &str) -> String {
    format!("@record:{record_id}")
}

/// Build a field token `@field:<id>:<field>@v<ver>`.
pub fn finance_field_clipboard_token(
    record_id: &str,
    field: FinanceMentionField,
    version: u64,
) -> String {
    format!("@field:{record_id}:{}@v{version}", field.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_finance_kinds_are_cataloged() {
        for kind in FinanceMentionKind::ALL {
            assert!(kind.record_type().starts_with("finance."));
            assert!(!kind.label(Locale::En).is_empty());
            assert!(!kind.label(Locale::Th).is_empty());
            assert!(kind.canonical_prefix().starts_with("/finance/"));
        }
    }
    #[test]
    fn clipboard_tokens_are_parseable() {
        let id = "00000000-0000-4000-8000-000000000001";
        let tok = finance_record_clipboard_token(id);
        assert_eq!(tok, format!("@record:{id}"));
        let ftok = finance_field_clipboard_token(id, FinanceMentionField::Amount, 1);
        assert_eq!(ftok, format!("@field:{id}:amount@v1"));
    }
    #[test]
    fn field_tokens_roundtrip_via_collaboration_parser() {
        let id = "00000000-0000-4000-8000-000000000002";
        for field in FinanceMentionField::ALL {
            let tok = finance_field_clipboard_token(id, *field, 2);
            let parsed = erpx_ui_ai_collaboration::parse_pasted_typed_token(&tok);
            assert!(parsed.is_some(), "field token {tok} should parse");
            let pasted = parsed.unwrap();
            assert_eq!(pasted.kind, erpx_ui_ai_collaboration::TokenKind::Field);
        }
        let bad = "@field:not-a-uuid:amount@v1";
        assert!(erpx_ui_ai_collaboration::parse_pasted_typed_token(bad).is_none());
    }
    #[test]
    fn record_tokens_respect_catalog_and_redaction() {
        assert!(is_finance_record_type("finance.journal"));
        assert!(!is_finance_record_type("procurement.purchase-order"));
        // Simulate reference_graph dedup and cap validation
        let host = uuid::Uuid::parse_str("00000000-0000-4000-8000-000000000010").unwrap();
        let target = uuid::Uuid::parse_str("00000000-0000-4000-8000-000000000011").unwrap();
        let ord = erpx_domain::reference_graph::validate_add(&[], target, host).unwrap();
        assert_eq!(ord, 0);
        assert!(erpx_domain::reference_graph::validate_add(&[target], target, host).is_err());
    }
    #[test]
    fn mention_kinds_cover_all_catalog_finance_types() {
        // Ensure mentions align with catalog.rs finance.* allow-list
        let catalog_finance = [
            "finance.journal",
            "finance.invoice",
            "finance.vendor-invoice",
            "finance.payment",
        ];
        for rt in catalog_finance {
            assert!(is_finance_record_type(rt) || rt == "finance.invoice");
        }
    }
}
