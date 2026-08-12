//! Target-neutral presentation contracts for POS command surfaces.
//!
//! The module is exported by the shared UI crate and is the target-neutral contract consumed by
//! POS presentation adapters. It describes the typed values a client may render or collect for
//! POS commands; consuming applications still submit through their authoritative API boundary
//! and perform authorization there. This module never makes authorization decisions and never
//! retains transport, database, payment-instrument, or provider material.

use std::{error::Error, fmt};

use erpx_ui_core::Locale;

/// Maximum bytes retained by any opaque POS identifier.
pub const MAX_OPAQUE_ID_BYTES: usize = 128;
/// Maximum bytes retained by a scalar presentation value.
pub const MAX_SCALAR_TEXT_BYTES: usize = 255;
/// Maximum number of lines in a sale-entry draft.
pub const MAX_SALE_ENTRY_LINES: usize = 100;

/// The two locales supported by the command presentation contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosCommandLocale {
    /// English copy.
    En,
    /// Thai copy.
    Th,
}

impl From<Locale> for PosCommandLocale {
    fn from(locale: Locale) -> Self {
        match locale {
            Locale::En => Self::En,
            Locale::Th => Self::Th,
        }
    }
}

impl From<PosCommandLocale> for Locale {
    fn from(locale: PosCommandLocale) -> Self {
        match locale {
            PosCommandLocale::En => Self::En,
            PosCommandLocale::Th => Self::Th,
        }
    }
}

/// A bounded opaque identifier safe to place in a presentation model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PosOpaqueId(String);

impl PosOpaqueId {
    /// Validate an identifier without interpreting its identifier scheme.
    pub fn new(value: impl Into<String>) -> Result<Self, PosCommandInputError> {
        let value = value.into();
        if value.is_empty() {
            return Err(PosCommandInputError::EmptyIdentifier);
        }
        if value.len() > MAX_OPAQUE_ID_BYTES {
            return Err(PosCommandInputError::IdentifierTooLong);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(PosCommandInputError::UnsafeIdentifier);
        }
        if matches!(value.as_str(), "." | "..") {
            return Err(PosCommandInputError::UnsafeIdentifier);
        }
        let lowered = value.to_ascii_lowercase();
        if [
            "api_key",
            "authorization",
            "bearer",
            "cookie",
            "credential",
            "password",
            "private_key",
            "secret",
            "session",
            "token",
        ]
        .iter()
        .any(|marker| lowered.contains(marker))
        {
            return Err(PosCommandInputError::SensitiveIdentifier);
        }
        Ok(Self(value))
    }

    /// Borrow the validated opaque value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A scalar, bounded, redacted value suitable for command presentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScalarText(String);

impl ScalarText {
    /// Validate a display/input value without accepting structured or sensitive material.
    pub fn new(value: impl Into<String>) -> Result<Self, PosCommandInputError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(PosCommandInputError::EmptyScalar);
        }
        if value.len() > MAX_SCALAR_TEXT_BYTES {
            return Err(PosCommandInputError::ScalarTooLong);
        }
        if value.chars().any(char::is_control)
            || value
                .chars()
                .any(|character| matches!(character, '{' | '}' | '[' | ']'))
        {
            return Err(PosCommandInputError::NonScalarText);
        }
        let lowered = value.to_ascii_lowercase();
        if [
            "api_key",
            "authorization",
            "bearer ",
            "cookie",
            "credential",
            "password",
            "private_key",
            "secret",
            "session",
            "token",
        ]
        .iter()
        .any(|marker| lowered.contains(marker))
        {
            return Err(PosCommandInputError::SensitiveScalar);
        }
        Ok(Self(value))
    }

    /// Borrow the validated scalar value.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated uppercase three-letter currency code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    /// Validate a presentation currency code.
    pub fn new(value: impl Into<String>) -> Result<Self, PosCommandInputError> {
        let value = value.into();
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(PosCommandInputError::InvalidCurrency);
        }
        Ok(Self(value))
    }

    /// Borrow the validated currency code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A non-negative amount in minor currency units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    currency: CurrencyCode,
    amount_minor: i64,
}

impl Money {
    /// Construct a bounded scalar money value.
    pub fn new(currency: CurrencyCode, amount_minor: i64) -> Result<Self, PosCommandInputError> {
        if amount_minor < 0 {
            return Err(PosCommandInputError::InvalidAmount);
        }
        Ok(Self {
            currency,
            amount_minor,
        })
    }

    /// Borrow the currency code.
    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }

    /// Return the amount in minor units.
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
}

/// Presentation labels for the four POS command surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosCommandKind {
    /// Enter a sale from scalar line values.
    SaleEntry,
    /// Review a proposed refund.
    RefundReview,
    /// Request receipt issuance for an existing sale.
    ReceiptIssuance,
    /// Review and approve a shift close.
    ShiftCloseApproval,
}

impl PosCommandKind {
    /// Return the localized command label.
    pub const fn label(self, locale: PosCommandLocale) -> &'static str {
        match (self, locale) {
            (Self::SaleEntry, PosCommandLocale::En) => "Sale entry",
            (Self::SaleEntry, PosCommandLocale::Th) => "บันทึกการขาย",
            (Self::RefundReview, PosCommandLocale::En) => "Refund review",
            (Self::RefundReview, PosCommandLocale::Th) => "ตรวจสอบการคืนเงิน",
            (Self::ReceiptIssuance, PosCommandLocale::En) => "Receipt issuance",
            (Self::ReceiptIssuance, PosCommandLocale::Th) => "ออกใบเสร็จ",
            (Self::ShiftCloseApproval, PosCommandLocale::En) => "Shift-close approval",
            (Self::ShiftCloseApproval, PosCommandLocale::Th) => "อนุมัติปิดกะ",
        }
    }

    /// Return the stable presentation route for the command surface.
    pub const fn route(self) -> &'static str {
        match self {
            Self::SaleEntry => "/pos/sales/entry",
            Self::RefundReview => "/pos/refunds/review",
            Self::ReceiptIssuance => "/pos/receipts/issue",
            Self::ShiftCloseApproval => "/pos/shift-close/approval",
        }
    }

    /// Resolve a canonical command route supplied by a target router.
    ///
    /// A single trailing slash is accepted for browser deep-link compatibility. Query strings,
    /// fragments, whitespace, and control characters are rejected before route matching so a
    /// sensitive value cannot be mistaken for a command route or echoed by an error.
    pub fn from_route(route: &str) -> Result<Self, PosCommandInputError> {
        if route.is_empty() {
            return Err(PosCommandInputError::InvalidCommandRoute);
        }
        if route
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
            || route.contains(['?', '#'])
        {
            return Err(PosCommandInputError::UnsafeCommandRoute);
        }
        let canonical = route.strip_suffix('/').unwrap_or(route);
        match canonical {
            "/pos/sales/entry" => Ok(Self::SaleEntry),
            "/pos/refunds/review" => Ok(Self::RefundReview),
            "/pos/receipts/issue" => Ok(Self::ReceiptIssuance),
            "/pos/shift-close/approval" => Ok(Self::ShiftCloseApproval),
            _ => Err(PosCommandInputError::InvalidCommandRoute),
        }
    }
}

impl TryFrom<&str> for PosCommandKind {
    type Error = PosCommandInputError;

    fn try_from(route: &str) -> Result<Self, Self::Error> {
        Self::from_route(route)
    }
}

/// The complete command surface registry, in navigation order.
pub const POS_COMMANDS: &[PosCommandKind] = &[
    PosCommandKind::SaleEntry,
    PosCommandKind::RefundReview,
    PosCommandKind::ReceiptIssuance,
    PosCommandKind::ShiftCloseApproval,
];

/// A typed line in a sale-entry form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleEntryLine {
    item_reference: ScalarText,
    description: ScalarText,
    quantity: u32,
    unit_price_minor: i64,
    tax_minor: i64,
}

impl SaleEntryLine {
    /// Construct a scalar-only sale-entry line.
    pub fn new(
        item_reference: ScalarText,
        description: ScalarText,
        quantity: u32,
        unit_price_minor: i64,
        tax_minor: i64,
    ) -> Result<Self, PosCommandInputError> {
        if quantity == 0 || unit_price_minor < 0 || tax_minor < 0 {
            return Err(PosCommandInputError::InvalidAmount);
        }
        i64::from(quantity)
            .checked_mul(unit_price_minor)
            .and_then(|subtotal| subtotal.checked_add(tax_minor))
            .ok_or(PosCommandInputError::InvalidAmount)?;
        Ok(Self {
            item_reference,
            description,
            quantity,
            unit_price_minor,
            tax_minor,
        })
    }

    /// Borrow the scalar item reference.
    pub fn item_reference(&self) -> &ScalarText {
        &self.item_reference
    }

    /// Borrow the scalar description.
    pub fn description(&self) -> &ScalarText {
        &self.description
    }

    /// Return the requested quantity.
    pub const fn quantity(&self) -> u32 {
        self.quantity
    }

    /// Return the unit price in minor units.
    pub const fn unit_price_minor(&self) -> i64 {
        self.unit_price_minor
    }

    /// Return tax in minor units.
    pub const fn tax_minor(&self) -> i64 {
        self.tax_minor
    }

    /// Return the reconciled line total in minor units.
    pub fn total_minor(&self) -> i64 {
        i64::from(self.quantity) * self.unit_price_minor + self.tax_minor
    }
}

/// A scalar-only sale-entry presentation input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleEntryInput {
    command_id: PosOpaqueId,
    register_id: PosOpaqueId,
    currency: CurrencyCode,
    lines: Vec<SaleEntryLine>,
    note: Option<ScalarText>,
}

impl SaleEntryInput {
    /// Construct a bounded sale-entry input.
    pub fn new(
        command_id: PosOpaqueId,
        register_id: PosOpaqueId,
        currency: CurrencyCode,
        lines: Vec<SaleEntryLine>,
        note: Option<ScalarText>,
    ) -> Result<Self, PosCommandInputError> {
        if lines.is_empty() {
            return Err(PosCommandInputError::NoSaleLines);
        }
        if lines.len() > MAX_SALE_ENTRY_LINES {
            return Err(PosCommandInputError::TooManySaleLines);
        }
        Ok(Self {
            command_id,
            register_id,
            currency,
            lines,
            note,
        })
    }

    /// Borrow the opaque command identifier.
    pub fn command_id(&self) -> &PosOpaqueId {
        &self.command_id
    }

    /// Borrow the opaque register identifier.
    pub fn register_id(&self) -> &PosOpaqueId {
        &self.register_id
    }

    /// Borrow the currency.
    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }

    /// Borrow scalar sale lines.
    pub fn lines(&self) -> &[SaleEntryLine] {
        &self.lines
    }

    /// Borrow the optional scalar note.
    pub fn note(&self) -> Option<&ScalarText> {
        self.note.as_ref()
    }
}

/// Sale-entry status shown in a redacted summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaleEntryStatus {
    /// Locally being edited.
    Draft,
    /// Waiting for authoritative review.
    Pending,
    /// Accepted by the command boundary.
    Accepted,
}

impl SaleEntryStatus {
    /// Return the localized status label.
    pub const fn label(self, locale: PosCommandLocale) -> &'static str {
        match (self, locale) {
            (Self::Draft, PosCommandLocale::En) => "Draft",
            (Self::Draft, PosCommandLocale::Th) => "ฉบับร่าง",
            (Self::Pending, PosCommandLocale::En) => "Pending review",
            (Self::Pending, PosCommandLocale::Th) => "รอตรวจสอบ",
            (Self::Accepted, PosCommandLocale::En) => "Accepted",
            (Self::Accepted, PosCommandLocale::Th) => "ยอมรับแล้ว",
        }
    }
}

/// A redacted scalar sale-entry summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaleEntrySummary {
    sale_id: PosOpaqueId,
    register_id: PosOpaqueId,
    line_count: usize,
    total: Money,
    status: SaleEntryStatus,
}

impl SaleEntrySummary {
    /// Construct a summary from already-validated scalar values.
    pub fn new(
        sale_id: PosOpaqueId,
        register_id: PosOpaqueId,
        line_count: usize,
        total: Money,
        status: SaleEntryStatus,
    ) -> Result<Self, PosCommandInputError> {
        if line_count == 0 || line_count > MAX_SALE_ENTRY_LINES {
            return Err(PosCommandInputError::InvalidLineCount);
        }
        Ok(Self {
            sale_id,
            register_id,
            line_count,
            total,
            status,
        })
    }

    /// Borrow the opaque sale identifier.
    pub fn sale_id(&self) -> &PosOpaqueId {
        &self.sale_id
    }

    /// Borrow the opaque register identifier.
    pub fn register_id(&self) -> &PosOpaqueId {
        &self.register_id
    }

    /// Return the number of lines.
    pub const fn line_count(&self) -> usize {
        self.line_count
    }

    /// Borrow the scalar total.
    pub fn total(&self) -> &Money {
        &self.total
    }

    /// Return the current summary status.
    pub const fn status(&self) -> SaleEntryStatus {
        self.status
    }
}

/// A scalar-only refund review input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundReviewInput {
    command_id: PosOpaqueId,
    sale_id: PosOpaqueId,
    line_reference: Option<PosOpaqueId>,
    amount: Money,
    reason: ScalarText,
}

impl RefundReviewInput {
    /// Construct a refund-review input without retaining instrument or provider details.
    pub fn new(
        command_id: PosOpaqueId,
        sale_id: PosOpaqueId,
        line_reference: Option<PosOpaqueId>,
        amount: Money,
        reason: ScalarText,
    ) -> Self {
        Self {
            command_id,
            sale_id,
            line_reference,
            amount,
            reason,
        }
    }

    /// Construct a refund-review input and enforce the positive amount required by the
    /// authoritative refund boundary.
    pub fn try_new(
        command_id: PosOpaqueId,
        sale_id: PosOpaqueId,
        line_reference: Option<PosOpaqueId>,
        amount: Money,
        reason: ScalarText,
    ) -> Result<Self, PosCommandInputError> {
        let input = Self::new(command_id, sale_id, line_reference, amount, reason);
        input.validate()?;
        Ok(input)
    }

    /// Validate the presentation input before handing it to an authenticated command adapter.
    pub fn validate(&self) -> Result<(), PosCommandInputError> {
        if self.amount.amount_minor() == 0 {
            return Err(PosCommandInputError::InvalidAmount);
        }
        Ok(())
    }

    /// Borrow the opaque command identifier.
    pub fn command_id(&self) -> &PosOpaqueId {
        &self.command_id
    }

    /// Borrow the opaque sale identifier.
    pub fn sale_id(&self) -> &PosOpaqueId {
        &self.sale_id
    }

    /// Borrow the optional opaque line reference.
    pub fn line_reference(&self) -> Option<&PosOpaqueId> {
        self.line_reference.as_ref()
    }

    /// Borrow the scalar refund amount.
    pub fn amount(&self) -> &Money {
        &self.amount
    }

    /// Borrow the scalar refund reason.
    pub fn reason(&self) -> &ScalarText {
        &self.reason
    }
}

/// Refund-review status shown to a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefundReviewStatus {
    /// Review has not been decided.
    Pending,
    /// Review was approved for server-side processing.
    Approved,
    /// Review was declined.
    Declined,
}

impl RefundReviewStatus {
    /// Return the localized status label.
    pub const fn label(self, locale: PosCommandLocale) -> &'static str {
        match (self, locale) {
            (Self::Pending, PosCommandLocale::En) => "Pending review",
            (Self::Pending, PosCommandLocale::Th) => "รอตรวจสอบ",
            (Self::Approved, PosCommandLocale::En) => "Approved",
            (Self::Approved, PosCommandLocale::Th) => "อนุมัติแล้ว",
            (Self::Declined, PosCommandLocale::En) => "Declined",
            (Self::Declined, PosCommandLocale::Th) => "ปฏิเสธ",
        }
    }
}

/// A redacted scalar refund-review summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundReviewSummary {
    refund_id: PosOpaqueId,
    sale_id: PosOpaqueId,
    amount: Money,
    status: RefundReviewStatus,
}

impl RefundReviewSummary {
    /// Construct a refund-review summary.
    pub fn new(
        refund_id: PosOpaqueId,
        sale_id: PosOpaqueId,
        amount: Money,
        status: RefundReviewStatus,
    ) -> Self {
        Self {
            refund_id,
            sale_id,
            amount,
            status,
        }
    }

    /// Construct a refund summary while preserving the positive-amount invariant.
    pub fn try_new(
        refund_id: PosOpaqueId,
        sale_id: PosOpaqueId,
        amount: Money,
        status: RefundReviewStatus,
    ) -> Result<Self, PosCommandInputError> {
        let summary = Self::new(refund_id, sale_id, amount, status);
        summary.validate()?;
        Ok(summary)
    }

    /// Validate the scalar summary before it is rendered.
    pub fn validate(&self) -> Result<(), PosCommandInputError> {
        if self.amount.amount_minor() == 0 {
            return Err(PosCommandInputError::InvalidAmount);
        }
        Ok(())
    }

    /// Borrow the opaque refund identifier.
    pub fn refund_id(&self) -> &PosOpaqueId {
        &self.refund_id
    }

    /// Borrow the opaque sale identifier.
    pub fn sale_id(&self) -> &PosOpaqueId {
        &self.sale_id
    }

    /// Borrow the scalar refund amount.
    pub fn amount(&self) -> &Money {
        &self.amount
    }

    /// Return the review status.
    pub const fn status(&self) -> RefundReviewStatus {
        self.status
    }
}

/// Redacted receipt destination choices. No contact or instrument value is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptDeliveryMode {
    /// Issue a printed copy at the register.
    Printed,
    /// Issue the customer-facing copy through the host application.
    CustomerCopy,
    /// Issue an internal store copy through the host application.
    StoreCopy,
}

impl ReceiptDeliveryMode {
    /// Return the localized destination label.
    pub const fn label(self, locale: PosCommandLocale) -> &'static str {
        match (self, locale) {
            (Self::Printed, PosCommandLocale::En) => "Printed copy",
            (Self::Printed, PosCommandLocale::Th) => "สำเนาพิมพ์",
            (Self::CustomerCopy, PosCommandLocale::En) => "Customer copy",
            (Self::CustomerCopy, PosCommandLocale::Th) => "สำเนาลูกค้า",
            (Self::StoreCopy, PosCommandLocale::En) => "Store copy",
            (Self::StoreCopy, PosCommandLocale::Th) => "สำเนาร้านค้า",
        }
    }
}

/// A scalar-only receipt issuance input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptIssueInput {
    command_id: PosOpaqueId,
    sale_id: PosOpaqueId,
    delivery_mode: ReceiptDeliveryMode,
}

impl ReceiptIssueInput {
    /// Construct a receipt issuance input.
    pub fn new(
        command_id: PosOpaqueId,
        sale_id: PosOpaqueId,
        delivery_mode: ReceiptDeliveryMode,
    ) -> Self {
        Self {
            command_id,
            sale_id,
            delivery_mode,
        }
    }

    /// Borrow the opaque command identifier.
    pub fn command_id(&self) -> &PosOpaqueId {
        &self.command_id
    }

    /// Borrow the opaque sale identifier.
    pub fn sale_id(&self) -> &PosOpaqueId {
        &self.sale_id
    }

    /// Return the redacted delivery mode.
    pub const fn delivery_mode(&self) -> ReceiptDeliveryMode {
        self.delivery_mode
    }
}

/// Receipt issuance status shown to a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptIssueStatus {
    /// Receipt issuance has been requested for review.
    Pending,
    /// Receipt issuance was accepted by the command boundary.
    Issued,
    /// Receipt issuance was declined.
    Declined,
}

impl ReceiptIssueStatus {
    /// Return the localized status label.
    pub const fn label(self, locale: PosCommandLocale) -> &'static str {
        match (self, locale) {
            (Self::Pending, PosCommandLocale::En) => "Pending issuance",
            (Self::Pending, PosCommandLocale::Th) => "รอออกใบเสร็จ",
            (Self::Issued, PosCommandLocale::En) => "Issued",
            (Self::Issued, PosCommandLocale::Th) => "ออกแล้ว",
            (Self::Declined, PosCommandLocale::En) => "Declined",
            (Self::Declined, PosCommandLocale::Th) => "ปฏิเสธ",
        }
    }
}

/// A redacted scalar receipt issuance summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiptIssueSummary {
    receipt_id: PosOpaqueId,
    sale_id: PosOpaqueId,
    delivery_mode: ReceiptDeliveryMode,
    status: ReceiptIssueStatus,
}

impl ReceiptIssueSummary {
    /// Construct a receipt issuance summary.
    pub fn new(
        receipt_id: PosOpaqueId,
        sale_id: PosOpaqueId,
        delivery_mode: ReceiptDeliveryMode,
        status: ReceiptIssueStatus,
    ) -> Self {
        Self {
            receipt_id,
            sale_id,
            delivery_mode,
            status,
        }
    }

    /// Borrow the opaque receipt identifier.
    pub fn receipt_id(&self) -> &PosOpaqueId {
        &self.receipt_id
    }

    /// Borrow the opaque sale identifier.
    pub fn sale_id(&self) -> &PosOpaqueId {
        &self.sale_id
    }

    /// Return the redacted delivery mode.
    pub const fn delivery_mode(&self) -> ReceiptDeliveryMode {
        self.delivery_mode
    }

    /// Return the issuance status.
    pub const fn status(&self) -> ReceiptIssueStatus {
        self.status
    }
}

/// A scalar-only shift-close approval input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShiftCloseApprovalInput {
    command_id: PosOpaqueId,
    shift_id: PosOpaqueId,
    expected_version: u64,
    counted_total: Money,
    note: Option<ScalarText>,
}

impl ShiftCloseApprovalInput {
    /// Construct a shift-close approval input.
    pub fn new(
        command_id: PosOpaqueId,
        shift_id: PosOpaqueId,
        expected_version: u64,
        counted_total: Money,
        note: Option<ScalarText>,
    ) -> Self {
        Self {
            command_id,
            shift_id,
            expected_version,
            counted_total,
            note,
        }
    }

    /// Borrow the opaque command identifier.
    pub fn command_id(&self) -> &PosOpaqueId {
        &self.command_id
    }

    /// Borrow the opaque shift identifier.
    pub fn shift_id(&self) -> &PosOpaqueId {
        &self.shift_id
    }

    /// Return the optimistic version displayed to the form.
    pub const fn expected_version(&self) -> u64 {
        self.expected_version
    }

    /// Borrow the scalar counted total.
    pub fn counted_total(&self) -> &Money {
        &self.counted_total
    }

    /// Borrow the optional scalar note.
    pub fn note(&self) -> Option<&ScalarText> {
        self.note.as_ref()
    }
}

/// Shift-close approval status shown to a client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShiftCloseStatus {
    /// Close is awaiting approval.
    Pending,
    /// Close was approved for server-side processing.
    Approved,
    /// Close was declined.
    Declined,
}

impl ShiftCloseStatus {
    /// Return the localized status label.
    pub const fn label(self, locale: PosCommandLocale) -> &'static str {
        match (self, locale) {
            (Self::Pending, PosCommandLocale::En) => "Pending approval",
            (Self::Pending, PosCommandLocale::Th) => "รออนุมัติ",
            (Self::Approved, PosCommandLocale::En) => "Approved",
            (Self::Approved, PosCommandLocale::Th) => "อนุมัติแล้ว",
            (Self::Declined, PosCommandLocale::En) => "Declined",
            (Self::Declined, PosCommandLocale::Th) => "ปฏิเสธ",
        }
    }
}

/// A redacted scalar shift-close summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShiftCloseSummary {
    shift_id: PosOpaqueId,
    counted_total: Money,
    status: ShiftCloseStatus,
}

impl ShiftCloseSummary {
    /// Construct a shift-close summary.
    pub fn new(shift_id: PosOpaqueId, counted_total: Money, status: ShiftCloseStatus) -> Self {
        Self {
            shift_id,
            counted_total,
            status,
        }
    }

    /// Borrow the opaque shift identifier.
    pub fn shift_id(&self) -> &PosOpaqueId {
        &self.shift_id
    }

    /// Borrow the scalar counted total.
    pub fn counted_total(&self) -> &Money {
        &self.counted_total
    }

    /// Return the shift-close status.
    pub const fn status(&self) -> ShiftCloseStatus {
        self.status
    }
}

/// Explicit presentation states for every POS command surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PosCommandState<T> {
    /// Data or command context is loading.
    Loading,
    /// The request succeeded without a command or summary to display.
    Empty,
    /// A command form or summary is available for presentation.
    Ready(T),
    /// The authoritative boundary denied the request.
    Denied { reason: ScalarText },
    /// A cached presentation is older than the freshness budget.
    Stale {
        data: Option<T>,
        age_seconds: u64,
        reason: ScalarText,
    },
    /// The current context is explicitly non-mutating.
    ReadOnly { data: Option<T>, reason: ScalarText },
    /// The command lost an optimistic concurrency race.
    Conflict { data: Option<T>, reason: ScalarText },
    /// The client cannot reach the authoritative boundary.
    Offline { data: Option<T>, reason: ScalarText },
    /// An otherwise unspecified presentation failure occurred.
    Error { data: Option<T>, reason: ScalarText },
}

impl<T> PosCommandState<T> {
    /// Return the explicit state kind for rendering.
    pub const fn kind(&self) -> PosCommandStateKind {
        match self {
            Self::Loading => PosCommandStateKind::Loading,
            Self::Empty => PosCommandStateKind::Empty,
            Self::Ready(_) => PosCommandStateKind::Ready,
            Self::Denied { .. } => PosCommandStateKind::Denied,
            Self::Stale { .. } => PosCommandStateKind::Stale,
            Self::ReadOnly { .. } => PosCommandStateKind::ReadOnly,
            Self::Conflict { .. } => PosCommandStateKind::Conflict,
            Self::Offline { .. } => PosCommandStateKind::Offline,
            Self::Error { .. } => PosCommandStateKind::Error,
        }
    }

    /// Return the localized state label.
    pub const fn state_label(&self, locale: PosCommandLocale) -> &'static str {
        self.kind().label(locale)
    }

    /// Return current or cached presentation data when a state carries it.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready(data) => Some(data),
            Self::Stale { data, .. }
            | Self::ReadOnly { data, .. }
            | Self::Conflict { data, .. }
            | Self::Offline { data, .. }
            | Self::Error { data, .. } => data.as_ref(),
            Self::Loading | Self::Empty | Self::Denied { .. } => None,
        }
    }

    /// Return a safe state explanation, if one is present.
    pub fn reason(&self) -> Option<&ScalarText> {
        match self {
            Self::Denied { reason }
            | Self::Stale { reason, .. }
            | Self::ReadOnly { reason, .. }
            | Self::Conflict { reason, .. }
            | Self::Offline { reason, .. }
            | Self::Error { reason, .. } => Some(reason),
            Self::Loading | Self::Empty | Self::Ready(_) => None,
        }
    }

    /// Return the freshness age when the state is explicitly stale.
    pub const fn age_seconds(&self) -> Option<u64> {
        match self {
            Self::Stale { age_seconds, .. } => Some(*age_seconds),
            Self::Loading
            | Self::Empty
            | Self::Ready(_)
            | Self::Denied { .. }
            | Self::ReadOnly { .. }
            | Self::Conflict { .. }
            | Self::Offline { .. }
            | Self::Error { .. } => None,
        }
    }

    /// Whether a safe payload can be rendered while preserving the explicit state label.
    pub fn can_render_data(&self) -> bool {
        self.data().is_some()
    }
}

/// State labels shared by all command surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosCommandStateKind {
    /// Loading state.
    Loading,
    /// Empty state.
    Empty,
    /// Ready state.
    Ready,
    /// Denied state.
    Denied,
    /// Stale state.
    Stale,
    /// Read-only state.
    ReadOnly,
    /// Conflict state.
    Conflict,
    /// Offline state.
    Offline,
    /// Error state.
    Error,
}

impl PosCommandStateKind {
    /// Return the localized state label.
    pub const fn label(self, locale: PosCommandLocale) -> &'static str {
        match (self, locale) {
            (Self::Loading, PosCommandLocale::En) => "Loading",
            (Self::Loading, PosCommandLocale::Th) => "กำลังโหลด",
            (Self::Empty, PosCommandLocale::En) => "Nothing to show",
            (Self::Empty, PosCommandLocale::Th) => "ไม่มีข้อมูล",
            (Self::Ready, PosCommandLocale::En) => "Ready",
            (Self::Ready, PosCommandLocale::Th) => "พร้อมใช้งาน",
            (Self::Denied, PosCommandLocale::En) => "Access denied",
            (Self::Denied, PosCommandLocale::Th) => "ไม่มีสิทธิ์เข้าถึง",
            (Self::Stale, PosCommandLocale::En) => "Stale data",
            (Self::Stale, PosCommandLocale::Th) => "ข้อมูลล้าสมัย",
            (Self::ReadOnly, PosCommandLocale::En) => "Read-only",
            (Self::ReadOnly, PosCommandLocale::Th) => "อ่านอย่างเดียว",
            (Self::Conflict, PosCommandLocale::En) => "Conflict",
            (Self::Conflict, PosCommandLocale::Th) => "ข้อมูลขัดแย้ง",
            (Self::Offline, PosCommandLocale::En) => "Offline",
            (Self::Offline, PosCommandLocale::Th) => "ออฟไลน์",
            (Self::Error, PosCommandLocale::En) => "Error",
            (Self::Error, PosCommandLocale::Th) => "เกิดข้อผิดพลาด",
        }
    }
}

/// Input validation failures that can be rendered without exposing source details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosCommandInputError {
    /// An opaque identifier was empty.
    EmptyIdentifier,
    /// An opaque identifier exceeded its bound.
    IdentifierTooLong,
    /// An opaque identifier contained unsafe characters.
    UnsafeIdentifier,
    /// An opaque identifier contained a sensitive marker.
    SensitiveIdentifier,
    /// A scalar value was empty.
    EmptyScalar,
    /// A scalar value exceeded its bound.
    ScalarTooLong,
    /// A scalar value was not scalar text.
    NonScalarText,
    /// A scalar value contained a sensitive marker.
    SensitiveScalar,
    /// A currency code was invalid.
    InvalidCurrency,
    /// An amount or arithmetic result was invalid.
    InvalidAmount,
    /// A sale had no lines.
    NoSaleLines,
    /// A sale exceeded the line bound.
    TooManySaleLines,
    /// A summary had an invalid line count.
    InvalidLineCount,
    /// A command route was not in the allow-list.
    InvalidCommandRoute,
    /// A command route contained query, fragment, whitespace, or control material.
    UnsafeCommandRoute,
}

impl fmt::Display for PosCommandInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyIdentifier => "POS command identifier is empty",
            Self::IdentifierTooLong => "POS command identifier is too long",
            Self::UnsafeIdentifier => "POS command identifier contains unsafe characters",
            Self::SensitiveIdentifier => "POS command identifier is sensitive",
            Self::EmptyScalar => "POS command scalar is empty",
            Self::ScalarTooLong => "POS command scalar is too long",
            Self::NonScalarText => "POS command value must be scalar text",
            Self::SensitiveScalar => "POS command scalar is sensitive",
            Self::InvalidCurrency => "POS command currency is invalid",
            Self::InvalidAmount => "POS command amount is invalid",
            Self::NoSaleLines => "POS sale entry has no lines",
            Self::TooManySaleLines => "POS sale entry has too many lines",
            Self::InvalidLineCount => "POS sale summary line count is invalid",
            Self::InvalidCommandRoute => "POS command route is invalid",
            Self::UnsafeCommandRoute => "POS command route contains unsafe material",
        })
    }
}

impl Error for PosCommandInputError {}
