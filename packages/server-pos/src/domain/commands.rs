//! Native POS command facts for register sessions, bills, voids and refunds.
//!
//! POS owns its shift, sale, line, fulfilment, refund, receipt and audit rows.
//! Cross-owner Finance, Inventory/Commerce and PMS effects are explicit ports;
//! this module never invents their SQL, stock movement or journal authority.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};

pub const OPERATION_SHIFT_OPEN: &str = "shift.open";
pub const OPERATION_SHIFT_CLOSE: &str = "shift.close";
pub const OPERATION_SHIFT_REVIEW: &str = "shift.review";
pub const OPERATION_CASH_APPROVE: &str = "cash.approve";
pub const OPERATION_SALE_CREATE: &str = "sale.create";
pub const OPERATION_SALE_HOLD: &str = "sale.hold";
pub const OPERATION_SALE_RESUME: &str = "sale.resume";
pub const OPERATION_SALE_ADD: &str = "sale.add";
pub const OPERATION_SALE_UPDATE_LINE: &str = "sale.update_line";
pub const OPERATION_SALE_CUSTOMER: &str = "sale.customer";
pub const OPERATION_SALE_MOVE_TABLE: &str = "sale.move_table";
pub const OPERATION_SALE_REMOVE: &str = "sale.remove";
pub const OPERATION_SALE_DISCOUNT: &str = "sale.discount";
pub const OPERATION_SALE_MERGE: &str = "sale.merge";
pub const OPERATION_SALE_SPLIT: &str = "sale.split";
pub const OPERATION_PAYMENT_CREATE: &str = "payment.create";
pub const OPERATION_PAYMENT_VERIFY_TRANSFER: &str = "payment.verify_transfer";
pub const OPERATION_PAYMENT_CANCEL_TRANSFER: &str = "payment.cancel_transfer";
pub const OPERATION_SALE_COMPLETE: &str = "sale.complete";
pub const OPERATION_SALE_CANCEL: &str = "sale.cancel";
pub const OPERATION_REFUND_REQUEST: &str = "refund.request";
pub const OPERATION_REFUND_APPROVE: &str = "refund.approve";
pub const OPERATION_REFUND_REJECT: &str = "refund.reject";

const AMOUNT_LIMIT: i64 = 99_999_999_999;
const REASON_MIN: usize = 3;
const REASON_MAX: usize = 1_000;
const DENOMINATIONS: [i64; 11] = [
    100_000, 50_000, 10_000, 5_000, 2_000, 1_000, 500, 200, 100, 50, 25,
];

#[derive(Clone, Debug)]
pub struct PosActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub department_id: Option<RecordId>,
    pub assignment_id: AssignmentId,
    pub permissions: Vec<String>,
    pub modules: Vec<String>,
    pub expires_at: i64,
}

impl PosActor {
    fn has(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }
    fn has_module(&self, module: &str) -> bool {
        self.modules.iter().any(|value| value == module)
    }
    fn require_scope(&self, now: TimestampMillis, permission: &str) -> Result<(), BusinessError> {
        if !self.has(permission) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "POS_PERMISSION_REQUIRED",
                "Your selected role does not grant this POS command.",
            ));
        }
        if !self.has_module("pos") {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "POS_MODULE_FORBIDDEN",
                "Your current role cannot access POS.",
            ));
        }
        if now.get() >= self.expires_at {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "WORK_CONTEXT_CHANGED",
                "Your selected work context changed. Review the current workspace and role.",
            ));
        }
        if self.department_id.is_some() {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "POS_BRANCH_REQUIRED",
                "Select a Company or branch for POS.",
            ));
        }
        Ok(())
    }
    pub fn require_sell(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_scope(now, "pos.sell")
    }
    pub fn require_approve(&self, now: TimestampMillis) -> Result<(), BusinessError> {
        self.require_scope(now, "pos.approve")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PosOperation {
    ShiftOpen,
    ShiftClose,
    ShiftReview,
    CashApprove,
    SaleCreate,
    SaleAdd,
    SaleUpdateLine,
    SaleCustomer,
    SaleMoveTable,
    SaleRemove,
    SaleDiscount,
    SaleMerge,
    SaleSplit,
    PaymentCreate,
    PaymentVerifyTransfer,
    PaymentCancelTransfer,
    SaleComplete,
    SaleHold,
    SaleResume,
    SaleCancel,
    RefundRequest,
    RefundApprove,
    RefundReject,
}

impl PosOperation {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            OPERATION_SHIFT_OPEN => Self::ShiftOpen,
            OPERATION_SHIFT_CLOSE => Self::ShiftClose,
            OPERATION_SHIFT_REVIEW => Self::ShiftReview,
            OPERATION_CASH_APPROVE => Self::CashApprove,
            OPERATION_SALE_CREATE => Self::SaleCreate,
            OPERATION_SALE_ADD => Self::SaleAdd,
            OPERATION_SALE_UPDATE_LINE => Self::SaleUpdateLine,
            OPERATION_SALE_CUSTOMER => Self::SaleCustomer,
            OPERATION_SALE_MOVE_TABLE => Self::SaleMoveTable,
            OPERATION_SALE_REMOVE => Self::SaleRemove,
            OPERATION_SALE_DISCOUNT => Self::SaleDiscount,
            OPERATION_SALE_MERGE => Self::SaleMerge,
            OPERATION_SALE_SPLIT => Self::SaleSplit,
            OPERATION_PAYMENT_CREATE => Self::PaymentCreate,
            OPERATION_PAYMENT_VERIFY_TRANSFER => Self::PaymentVerifyTransfer,
            OPERATION_PAYMENT_CANCEL_TRANSFER => Self::PaymentCancelTransfer,
            OPERATION_SALE_COMPLETE => Self::SaleComplete,
            OPERATION_SALE_HOLD => Self::SaleHold,
            OPERATION_SALE_RESUME => Self::SaleResume,
            OPERATION_SALE_CANCEL => Self::SaleCancel,
            OPERATION_REFUND_REQUEST => Self::RefundRequest,
            OPERATION_REFUND_APPROVE => Self::RefundApprove,
            OPERATION_REFUND_REJECT => Self::RefundReject,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ShiftOpen => OPERATION_SHIFT_OPEN,
            Self::ShiftClose => OPERATION_SHIFT_CLOSE,
            Self::ShiftReview => OPERATION_SHIFT_REVIEW,
            Self::CashApprove => OPERATION_CASH_APPROVE,
            Self::SaleCreate => OPERATION_SALE_CREATE,
            Self::SaleAdd => OPERATION_SALE_ADD,
            Self::SaleUpdateLine => OPERATION_SALE_UPDATE_LINE,
            Self::SaleCustomer => OPERATION_SALE_CUSTOMER,
            Self::SaleMoveTable => OPERATION_SALE_MOVE_TABLE,
            Self::SaleRemove => OPERATION_SALE_REMOVE,
            Self::SaleDiscount => OPERATION_SALE_DISCOUNT,
            Self::SaleMerge => OPERATION_SALE_MERGE,
            Self::SaleSplit => OPERATION_SALE_SPLIT,
            Self::PaymentCreate => OPERATION_PAYMENT_CREATE,
            Self::PaymentVerifyTransfer => OPERATION_PAYMENT_VERIFY_TRANSFER,
            Self::PaymentCancelTransfer => OPERATION_PAYMENT_CANCEL_TRANSFER,
            Self::SaleComplete => OPERATION_SALE_COMPLETE,
            Self::SaleHold => OPERATION_SALE_HOLD,
            Self::SaleResume => OPERATION_SALE_RESUME,
            Self::SaleCancel => OPERATION_SALE_CANCEL,
            Self::RefundRequest => OPERATION_REFUND_REQUEST,
            Self::RefundApprove => OPERATION_REFUND_APPROVE,
            Self::RefundReject => OPERATION_REFUND_REJECT,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftOpenInput {
    pub register_id: String,
    pub opening_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftCloseInput {
    pub id: String,
    pub expected_version: i64,
    pub counted_minor: i64,
    pub denominations: Option<Vec<(i64, i64)>>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShiftReviewInput {
    pub id: String,
    pub expected_version: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CashApproveInput {
    pub id: String,
    pub expected_version: i64,
    pub movement_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleCreateInput {
    pub shift_id: String,
    pub table_id: Option<String>,
    pub guests: i64,
    pub party_id: Option<String>,
    pub service_mode: String,
    pub currency: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleAddInput {
    pub id: String,
    pub expected_version: i64,
    pub product_id: String,
    pub quantity_milli: i64,
    pub modifier_ids: Vec<String>,
    pub discount_minor: i64,
    pub note: String,
    pub expected_price_minor: Option<i64>,
    pub expected_catalog_version: Option<i64>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleUpdateLineInput {
    pub id: String,
    pub expected_version: i64,
    pub line_id: String,
    pub quantity_milli: i64,
    pub discount_minor: i64,
    pub note: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleCustomerInput {
    pub id: String,
    pub expected_version: i64,
    pub party_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleMoveTableInput {
    pub id: String,
    pub expected_version: i64,
    pub table_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleRemoveInput {
    pub id: String,
    pub expected_version: i64,
    pub line_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleDiscountInput {
    pub id: String,
    pub expected_version: i64,
    pub amount_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleMergeInput {
    pub id: String,
    pub expected_version: i64,
    pub target_id: String,
    pub target_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleSplitInput {
    pub id: String,
    pub expected_version: i64,
    pub line_id: String,
    pub quantity_milli: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentCreateInput {
    pub id: String,
    pub expected_version: i64,
    pub method: String,
    pub amount_minor: i64,
    pub tendered_minor: i64,
    pub reference: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentVerifyTransferInput {
    pub id: String,
    pub expected_version: i64,
    pub bank_transaction_id: String,
    pub bank_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentCancelTransferInput {
    pub id: String,
    pub expected_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaleVersionInput {
    pub id: String,
    pub expected_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundLineInput {
    pub line_id: String,
    pub quantity_milli: i64,
    pub restock: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundRequestInput {
    pub id: String,
    pub expected_version: i64,
    pub reason: String,
    pub lines: Vec<RefundLineInput>,
    pub amount_minor: Option<i64>,
    pub payout_shift_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundApproveInput {
    pub id: String,
    pub expected_version: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundRejectInput {
    pub id: String,
    pub expected_version: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PosCommandInput {
    ShiftOpen(ShiftOpenInput),
    ShiftClose(ShiftCloseInput),
    ShiftReview(ShiftReviewInput),
    CashApprove(CashApproveInput),
    SaleCreate(SaleCreateInput),
    SaleAdd(SaleAddInput),
    SaleUpdateLine(SaleUpdateLineInput),
    SaleCustomer(SaleCustomerInput),
    SaleMoveTable(SaleMoveTableInput),
    SaleRemove(SaleRemoveInput),
    SaleDiscount(SaleDiscountInput),
    SaleMerge(SaleMergeInput),
    SaleSplit(SaleSplitInput),
    PaymentCreate(PaymentCreateInput),
    PaymentVerifyTransfer(PaymentVerifyTransferInput),
    PaymentCancelTransfer(PaymentCancelTransferInput),
    SaleComplete(SaleVersionInput),
    SaleVersion(SaleVersionInput),
    RefundRequest(RefundRequestInput),
    RefundApprove(RefundApproveInput),
    RefundReject(RefundRejectInput),
}

#[derive(Clone, Debug)]
pub struct PosCommand {
    pub key: CommandKey,
    pub operation: PosOperation,
    pub input: PosCommandInput,
    pub canonical_input: String,
}

impl PosCommand {
    pub fn operation(&self) -> &'static str {
        self.operation.as_str()
    }
    pub fn receipt_input(&self) -> &str {
        &self.canonical_input
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosSettings {
    pub enabled: bool,
    pub currency: String,
    pub cashier_discount_bps: i64,
    pub accounts_json: String,
    pub accounting_approved_by: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosRegister {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub name: String,
    pub mode: String,
    pub active: bool,
    pub property_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosShift {
    pub id: String,
    pub register_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub business_date: String,
    pub opened_by: String,
    pub opening_minor: i64,
    pub state: String,
    pub counted_minor: Option<i64>,
    pub expected_minor: Option<i64>,
    pub variance_minor: Option<i64>,
    pub closed_by: Option<String>,
    pub version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosCashMovement {
    pub id: String,
    pub shift_id: String,
    pub amount_minor: i64,
    pub created_by: String,
    pub approved_by: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosSale {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub register_id: String,
    pub shift_id: String,
    pub number: String,
    pub party_id: Option<String>,
    pub issuer_json: String,
    pub currency: String,
    pub business_date: String,
    pub service_mode: String,
    pub state: String,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub discount_minor: i64,
    pub total_minor: i64,
    pub created_by: String,
    pub version: i64,
    pub accounts_json: String,
    pub discount_approved_by: Option<String>,
    pub invoice_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosPaymentInsert {
    pub id: String,
    pub sale_id: String,
    pub company_id: String,
    pub method: String,
    pub amount_minor: i64,
    pub tendered_minor: i64,
    pub reference: String,
    pub created_by: String,
    pub now: TimestampMillis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosReceiptRequest {
    pub pos_payment_id: String,
    pub finance_payment_id: String,
    pub company_id: CompanyId,
    pub party_id: Option<String>,
    pub number: String,
    pub payment_date: String,
    pub currency: String,
    pub amount_minor: i64,
    pub account_id: String,
    pub journal_id: String,
    pub actor_id: UserId,
    pub now: TimestampMillis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosStockRow {
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub reserved_milli: i64,
    pub value_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosPayment {
    pub id: String,
    pub sale_id: String,
    pub method: String,
    pub state: String,
    pub amount_minor: i64,
    pub reference: String,
    pub created_by: String,
    pub version: i64,
    pub finance_payment_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosBankTransaction {
    pub id: String,
    pub version: i64,
    pub match_status: String,
    pub bank_account: String,
    pub currency: String,
    pub amount_minor: i64,
    pub external_id: String,
    pub transaction_date: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosRefundDetail {
    pub id: String,
    pub sale_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub kind: String,
    pub state: String,
    pub lines_json: String,
    pub amount_minor: i64,
    pub tax_minor: i64,
    pub accounting_date: String,
    pub requested_by: String,
    pub payout_shift_id: Option<String>,
    pub version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosPaymentRefundRow {
    pub id: String,
    pub method: String,
    pub amount_minor: i64,
    pub refunded_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosRefundLineRow {
    pub id: String,
    pub fulfillment_id: String,
    pub quantity_milli: i64,
    pub returned_milli: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosFulfillmentReturnFacts {
    pub fulfillment_id: String,
    pub kind: String,
    pub state: String,
    pub quantity_milli: i64,
    pub cost_minor: i64,
    pub warehouse_id: String,
    pub product_id: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosReturnPart {
    pub fulfillment_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub expected_value_minor: i64,
}
#[derive(Clone, Debug)]
pub struct PosRefundPaymentRequest {
    pub finance_payment_id: String,
    pub company_id: CompanyId,
    pub party_id: Option<String>,
    pub number: String,
    pub payment_date: String,
    pub currency: String,
    pub amount_minor: i64,
    pub account_id: String,
    pub journal_id: String,
    pub actor_id: UserId,
    pub now: TimestampMillis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosConsumptionPart {
    pub fulfillment_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub expected_value_minor: i64,
}
#[derive(Clone, Debug)]
pub struct PosInvoiceRequest {
    pub invoice_id: String,
    pub company_id: CompanyId,
    pub party_id: Option<String>,
    pub number: String,
    pub invoice_date: String,
    pub currency: String,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
    pub receivable_account_id: String,
    pub revenue_account_id: String,
    pub tax_account_id: String,
    pub journal_id: String,
    pub sale_id: String,
    pub actor_id: UserId,
    pub now: TimestampMillis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosFulfillmentStock {
    pub id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub kind: String,
    pub recipe_json: String,
    pub state: String,
    pub reserved: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosCatalogSnapshot {
    pub register_id: String,
    pub product: PosCatalogProduct,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosCatalogProduct {
    pub id: String,
    pub name: String,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub tax_inclusive: bool,
    pub kind: String,
    pub station: String,
    pub recipe_json: String,
    pub modifiers_json: String,
    pub catalog_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosFulfillmentInsert {
    pub id: String,
    pub company_id: String,
    pub branch_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub kind: String,
    pub station: String,
    pub note: String,
    pub recipe_json: String,
    pub accounts_json: String,
    pub currency: String,
    pub business_date: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosLineInsert {
    pub id: String,
    pub sale_id: String,
    pub fulfillment_id: String,
    pub product_id: String,
    pub name: String,
    pub quantity_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub tax_inclusive: bool,
    pub discount_minor: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub modifiers_json: String,
    pub note: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosLineDiscountRow {
    pub id: String,
    pub quantity_milli: i64,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub tax_inclusive: bool,
    pub discount_minor: i64,
    pub bill_discount_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosLineSplitRow {
    pub id: String,
    pub fulfillment_id: String,
    pub quantity_milli: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub discount_minor: i64,
    pub bill_discount_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosFulfillmentSplitFacts {
    pub kind: String,
    pub state: String,
    pub reserved: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosLineDetail {
    pub id: String,
    pub fulfillment_id: String,
    pub price_minor: i64,
    pub tax_bps: i64,
    pub tax_inclusive: bool,
    pub discount_minor: i64,
    pub bill_discount_minor: i64,
    pub note: String,
    pub fulfillment_state: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosLine {
    pub id: String,
    pub sale_id: String,
    pub quantity_milli: i64,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub returned_milli: i64,
    pub price_minor: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosFulfillment {
    pub id: String,
    pub state: String,
    pub kind: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosRefund {
    pub id: String,
    pub sale_id: String,
    pub company_id: String,
    pub branch_id: String,
    pub state: String,
    pub requested_by: String,
    pub version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosReceipt {
    pub request_hash: String,
    pub result_json: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosJournalLine {
    pub account_id: String,
    pub debit: i64,
    pub credit: i64,
}
#[derive(Clone, Debug)]
pub struct PosJournalRequest {
    pub company_id: CompanyId,
    pub branch_id: Option<RecordId>,
    pub chart_id: String,
    pub currency: String,
    pub business_date: String,
    pub source_key: String,
    pub source_id: String,
    pub actor_id: UserId,
    pub lines: Vec<PosJournalLine>,
    pub now: TimestampMillis,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PosResult {
    pub id: String,
    pub version: Option<i64>,
}

impl PosResult {
    pub fn to_wire(&self) -> serde_json::Value {
        match self.version {
            Some(version) => serde_json::json!({"id": self.id, "version": version}),
            None => serde_json::json!({"id": self.id}),
        }
    }
}

pub(crate) fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "POS_NOT_FOUND",
        "Record not found in this workspace.",
    )
}
pub(crate) fn version_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "POS_VERSION_CONFLICT",
        "Record changed. Load the latest data and try again.",
    )
}
pub(crate) fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_SOURCE_INVALID",
        "The current POS source could not be verified.",
    )
}
pub(crate) fn idempotency_conflict() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "IDEMPOTENCY_CONFLICT",
        "Command number is already used for other data.",
    )
}
pub(crate) fn request_invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}
pub(crate) fn conflict(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(FailureKind::Conflict, code, message)
}
pub(crate) fn independent_review() -> BusinessError {
    conflict(
        "POS_INDEPENDENT_REVIEW",
        "A different manager must review this POS record.",
    )
}
pub(crate) fn accounting_review_required() -> BusinessError {
    conflict(
        "POS_ACCOUNTING_REVIEW_REQUIRED",
        "Finance must approve the POS posting accounts before selling.",
    )
}
pub(crate) fn setup_required() -> BusinessError {
    conflict(
        "POS_SETUP_REQUIRED",
        "Configure POS accounts and sales points before use.",
    )
}
pub(crate) fn day_closed() -> BusinessError {
    conflict(
        "POS_DAY_CLOSED",
        "This business day is preparing to close. Use a current-day adjustment.",
    )
}
pub(crate) fn period_closed() -> BusinessError {
    conflict(
        "POS_PERIOD_CLOSED",
        "An open accounting period is required for this date.",
    )
}

pub fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}
pub fn valid_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}
pub fn js_trim(value: &str) -> &str {
    value.trim_matches(|character: char| {
        matches!(character, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
    })
}
fn reason(value: &str) -> Result<String, BusinessError> {
    let value = js_trim(value);
    if !(REASON_MIN..=REASON_MAX).contains(&value.chars().count()) {
        return Err(request_invalid(
            "POS_COMMAND_INVALID",
            "Enter a reason between 3 and 1000 characters.",
        ));
    }
    Ok(value.to_owned())
}
fn money(value: i64) -> Result<i64, BusinessError> {
    if !(0..=AMOUNT_LIMIT).contains(&value) {
        return Err(request_invalid(
            "POS_COMMAND_INVALID",
            "Enter a valid POS money amount.",
        ));
    }
    Ok(value)
}
fn version(value: i64) -> Result<i64, BusinessError> {
    if value < 1 {
        return Err(version_conflict());
    }
    Ok(value)
}
fn identifier(value: &str) -> Result<String, BusinessError> {
    if value.is_empty() || value.len() > 200 {
        return Err(request_invalid(
            "POS_COMMAND_INVALID",
            "Choose a current POS record.",
        ));
    }
    Ok(value.to_owned())
}
fn text(value: &serde_json::Value, name: &str) -> Result<String, BusinessError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| request_invalid("POS_COMMAND_INVALID", "Review the POS command fields."))
        .and_then(identifier)
}
fn optional_text(value: &serde_json::Value, name: &str) -> Result<Option<String>, BusinessError> {
    match value.get(name) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) => identifier(value).map(Some),
        _ => Err(request_invalid(
            "POS_COMMAND_INVALID",
            "Review the POS command fields.",
        )),
    }
}
fn integer(value: &serde_json::Value, name: &str) -> Result<i64, BusinessError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| request_invalid("POS_COMMAND_INVALID", "Review the POS command fields."))
}
fn boolean(value: &serde_json::Value, name: &str) -> Result<bool, BusinessError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| request_invalid("POS_COMMAND_INVALID", "Review the POS command fields."))
}
fn object<'a>(
    value: &'a serde_json::Value,
    allowed: &[&str],
    required: &[&str],
) -> Result<&'a serde_json::Map<String, serde_json::Value>, BusinessError> {
    let Some(object) = value.as_object() else {
        return Err(request_invalid(
            "POS_COMMAND_INVALID",
            "Use a POS command object.",
        ));
    };
    if !object.keys().all(|key| allowed.contains(&key.as_str()))
        || !required.iter().all(|key| object.contains_key(*key))
    {
        return Err(request_invalid(
            "POS_COMMAND_INVALID",
            "Review the exact POS command fields.",
        ));
    }
    Ok(object)
}
fn attachments(value: &serde_json::Value) -> Result<(), BusinessError> {
    match value.get("attachmentSetIds") {
        None => Ok(()),
        Some(serde_json::Value::Array(values)) if values.is_empty() => Ok(()),
        Some(serde_json::Value::Array(values))
            if values.len() <= 20
                && values
                    .iter()
                    .all(|value| value.as_str().is_some_and(|value| !value.is_empty())) =>
        {
            Err(request_invalid(
                "POS_ATTACHMENT_NATIVE_PORT_REQUIRED",
                "Native POS attachment binding is not available yet.",
            ))
        }
        _ => Err(request_invalid(
            "POS_ATTACHMENT_INPUT",
            "Choose evidence for the current record revision.",
        )),
    }
}

fn canonical(values: &[(&str, String)]) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| format!("{}:{value}", serde_json::to_string(key).unwrap()))
            .collect::<Vec<_>>()
            .join(",")
    )
}
fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap()
}
fn optional_money(value: &serde_json::Value, key: &str) -> Result<Option<i64>, BusinessError> {
    match value.get(key) {
        None => Ok(None),
        Some(value) => Ok(Some(money(value.as_i64().ok_or_else(|| {
            request_invalid("POS_COMMAND_INVALID", "Review the POS money fields.")
        })?)?)),
    }
}
fn optional_note(value: &serde_json::Value, key: &str) -> Result<String, BusinessError> {
    match value.get(key) {
        None => Ok(String::new()),
        Some(value) => Ok(value
            .as_str()
            .filter(|value| value.len() <= 500)
            .ok_or_else(|| request_invalid("POS_COMMAND_INVALID", "Review the line note."))?
            .to_owned()),
    }
}
fn optional_json(value: Option<&str>) -> String {
    value.map_or_else(|| "null".into(), json_string)
}
fn lines_json(lines: &[RefundLineInput]) -> String {
    format!(
        "[{}]",
        lines
            .iter()
            .map(|line| format!(
                "{{\"lineId\":{},\"quantityMilli\":{},\"restock\":{}}}",
                json_string(&line.line_id),
                line.quantity_milli,
                line.restock
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

impl PosCommand {
    pub fn parse(
        operation: PosOperation,
        key: CommandKey,
        value: &serde_json::Value,
    ) -> Result<Self, BusinessError> {
        let (input, canonical_input) = match operation {
            PosOperation::ShiftOpen => {
                object(
                    value,
                    &["registerId", "openingMinor"],
                    &["registerId", "openingMinor"],
                )?;
                let register_id = text(value, "registerId")?;
                let opening_minor = money(integer(value, "openingMinor")?)?;
                (
                    PosCommandInput::ShiftOpen(ShiftOpenInput {
                        register_id: register_id.clone(),
                        opening_minor,
                    }),
                    canonical(&[
                        ("registerId", json_string(&register_id)),
                        ("openingMinor", opening_minor.to_string()),
                    ]),
                )
            }
            PosOperation::ShiftClose => {
                object(
                    value,
                    &["id", "expectedVersion", "countedMinor", "denominations"],
                    &["id", "expectedVersion", "countedMinor"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let counted_minor = money(integer(value, "countedMinor")?)?;
                let denominations = match value.get("denominations") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::Object(entries)) => {
                        let mut values = Vec::new();
                        for (key, value) in entries {
                            let denomination = key.parse::<i64>().map_err(|_| {
                                request_invalid(
                                    "POS_CASH_COUNT",
                                    "Review the counted denominations.",
                                )
                            })?;
                            let quantity = value.as_i64().ok_or_else(|| {
                                request_invalid(
                                    "POS_CASH_COUNT",
                                    "Review the counted denominations.",
                                )
                            })?;
                            if !DENOMINATIONS.contains(&denomination)
                                || !(0..=100_000).contains(&quantity)
                            {
                                return Err(request_invalid(
                                    "POS_CASH_COUNT",
                                    "Review the counted denominations.",
                                ));
                            }
                            values.push((denomination, quantity));
                        }
                        values.sort_by_key(|(denomination, _)| *denomination);
                        Some(values)
                    }
                    _ => {
                        return Err(request_invalid(
                            "POS_CASH_COUNT",
                            "Review the counted denominations.",
                        ));
                    }
                };
                if let Some(ref values) = denominations {
                    let total: i64 = values
                        .iter()
                        .try_fold(0i64, |total, (denomination, quantity)| {
                            total.checked_add(denomination.checked_mul(*quantity)?)
                        })
                        .ok_or_else(|| {
                            request_invalid("POS_CASH_COUNT", "Review the counted denominations.")
                        })?;
                    if total != counted_minor {
                        return Err(request_invalid(
                            "POS_CASH_COUNT",
                            "Denomination counts must match the counted amount.",
                        ));
                    }
                }
                let denomination_json = denominations.as_ref().map_or_else(
                    || "null".into(),
                    |values| {
                        format!(
                            "{{{}}}",
                            values
                                .iter()
                                .map(|(denomination, quantity)| format!(
                                    "{}:{}",
                                    json_string(&denomination.to_string()),
                                    quantity
                                ))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    },
                );
                (
                    PosCommandInput::ShiftClose(ShiftCloseInput {
                        id: id.clone(),
                        expected_version,
                        counted_minor,
                        denominations,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("countedMinor", counted_minor.to_string()),
                        ("denominations", denomination_json),
                    ]),
                )
            }
            PosOperation::ShiftReview => {
                object(
                    value,
                    &["id", "expectedVersion", "reason", "attachmentSetIds"],
                    &["id", "expectedVersion", "reason"],
                )?;
                attachments(value)?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let reason = reason(&text(value, "reason")?)?;
                (
                    PosCommandInput::ShiftReview(ShiftReviewInput {
                        id: id.clone(),
                        expected_version,
                        reason: reason.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("reason", json_string(&reason)),
                    ]),
                )
            }
            PosOperation::CashApprove => {
                object(
                    value,
                    &["id", "expectedVersion", "movementId"],
                    &["id", "expectedVersion", "movementId"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let movement_id = text(value, "movementId")?;
                (
                    PosCommandInput::CashApprove(CashApproveInput {
                        id: id.clone(),
                        expected_version,
                        movement_id: movement_id.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("movementId", json_string(&movement_id)),
                    ]),
                )
            }
            PosOperation::SaleCreate => {
                object(
                    value,
                    &[
                        "shiftId",
                        "tableId",
                        "guests",
                        "partyId",
                        "serviceMode",
                        "currency",
                    ],
                    &["shiftId", "guests", "serviceMode"],
                )?;
                let shift_id = text(value, "shiftId")?;
                let table_id = optional_text(value, "tableId")?;
                let guests = integer(value, "guests")?;
                if !(1..=100).contains(&guests) {
                    return Err(request_invalid(
                        "POS_COMMAND_INVALID",
                        "Choose between 1 and 100 guests.",
                    ));
                }
                let party_id = optional_text(value, "partyId")?;
                let service_mode = text(value, "serviceMode")?;
                if !["retail", "dine_in", "takeaway"].contains(&service_mode.as_str()) {
                    return Err(request_invalid(
                        "POS_MODE_INVALID",
                        "Mode does not match the sales point.",
                    ));
                }
                let currency = optional_text(value, "currency")?;
                if let Some(ref currency) = currency
                    && (currency.len() != 3
                        || !currency.bytes().all(|byte| byte.is_ascii_uppercase()))
                {
                    return Err(request_invalid(
                        "POS_COMMAND_INVALID",
                        "Review the POS currency.",
                    ));
                }
                (
                    PosCommandInput::SaleCreate(SaleCreateInput {
                        shift_id: shift_id.clone(),
                        table_id: table_id.clone(),
                        guests,
                        party_id: party_id.clone(),
                        service_mode: service_mode.clone(),
                        currency: currency.clone(),
                    }),
                    canonical(&[
                        ("shiftId", json_string(&shift_id)),
                        ("tableId", optional_json(table_id.as_deref())),
                        ("guests", guests.to_string()),
                        ("partyId", optional_json(party_id.as_deref())),
                        ("serviceMode", json_string(&service_mode)),
                        ("currency", optional_json(currency.as_deref())),
                    ]),
                )
            }
            PosOperation::SaleAdd => {
                object(
                    value,
                    &[
                        "id",
                        "expectedVersion",
                        "productId",
                        "quantityMilli",
                        "modifierIds",
                        "discountMinor",
                        "note",
                        "expectedPriceMinor",
                        "expectedCatalogVersion",
                    ],
                    &["id", "expectedVersion", "productId", "quantityMilli"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let product_id = text(value, "productId")?;
                let quantity_milli = integer(value, "quantityMilli")?;
                if !(1..=999_999_999).contains(&quantity_milli) {
                    return Err(request_invalid("POS_COMMAND_INVALID", "Review the line quantity."));
                }
                let modifier_ids = match value.get("modifierIds") {
                    None => Vec::new(),
                    Some(serde_json::Value::Array(entries)) => {
                        let mut ids = Vec::new();
                        for entry in entries {
                            let id = entry
                                .as_str()
                                .filter(|value| !value.is_empty() && value.len() <= 128)
                                .ok_or_else(|| {
                                    request_invalid(
                                        "POS_MODIFIER_INVALID",
                                        "Review the selected product options.",
                                    )
                                })?;
                            if !ids.iter().any(|current| current == id) {
                                ids.push(id.to_owned());
                            }
                        }
                        if ids.len() > 30 {
                            return Err(request_invalid(
                                "POS_MODIFIER_INVALID",
                                "Review the selected product options.",
                            ));
                        }
                        ids
                    }
                    _ => {
                        return Err(request_invalid(
                            "POS_MODIFIER_INVALID",
                            "Review the selected product options.",
                        ));
                    }
                };
                let discount_minor = optional_money(value, "discountMinor")?.unwrap_or(0);
                let note = optional_note(value, "note")?;
                let expected_price_minor = optional_money(value, "expectedPriceMinor")?;
                let expected_catalog_version = match value.get("expectedCatalogVersion") {
                    None => None,
                    Some(value) => Some(version(value.as_i64().ok_or_else(|| {
                        request_invalid("POS_CATALOG_CHANGED", "Review the current catalogue.")
                    })?)?),
                };
                (
                    PosCommandInput::SaleAdd(SaleAddInput {
                        id: id.clone(),
                        expected_version,
                        product_id: product_id.clone(),
                        quantity_milli,
                        modifier_ids: modifier_ids.clone(),
                        discount_minor,
                        note: note.clone(),
                        expected_price_minor,
                        expected_catalog_version,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("productId", json_string(&product_id)),
                        ("quantityMilli", quantity_milli.to_string()),
                        (
                            "modifierIds",
                            format!(
                                "[{}]",
                                modifier_ids.iter().map(|id| json_string(id)).collect::<Vec<_>>().join(",")
                            ),
                        ),
                        ("discountMinor", discount_minor.to_string()),
                        ("note", json_string(&note)),
                        (
                            "expectedPriceMinor",
                            optional_json(expected_price_minor.map(|v| v.to_string()).as_deref()),
                        ),
                        (
                            "expectedCatalogVersion",
                            optional_json(expected_catalog_version.map(|v| v.to_string()).as_deref()),
                        ),
                    ]),
                )
            }
            PosOperation::SaleUpdateLine => {
                object(
                    value,
                    &["id", "expectedVersion", "lineId", "quantityMilli", "discountMinor", "note"],
                    &["id", "expectedVersion", "lineId", "quantityMilli", "discountMinor"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let line_id = text(value, "lineId")?;
                let quantity_milli = integer(value, "quantityMilli")?;
                if !(1..=999_999_999).contains(&quantity_milli) {
                    return Err(request_invalid("POS_COMMAND_INVALID", "Review the line quantity."));
                }
                let discount_minor = money(integer(value, "discountMinor")?)?;
                let note = optional_note(value, "note")?;
                (
                    PosCommandInput::SaleUpdateLine(SaleUpdateLineInput {
                        id: id.clone(),
                        expected_version,
                        line_id: line_id.clone(),
                        quantity_milli,
                        discount_minor,
                        note: note.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("lineId", json_string(&line_id)),
                        ("quantityMilli", quantity_milli.to_string()),
                        ("discountMinor", discount_minor.to_string()),
                        ("note", json_string(&note)),
                    ]),
                )
            }
            PosOperation::SaleCustomer => {
                object(
                    value,
                    &["id", "expectedVersion", "partyId"],
                    &["id", "expectedVersion"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let party_id = match value.get("partyId") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(value) => Some(
                        value
                            .as_str()
                            .filter(|value| !value.is_empty() && value.len() <= 128)
                            .ok_or_else(|| {
                                request_invalid("POS_CUSTOMER_INVALID", "Customer is not available.")
                            })?
                            .to_owned(),
                    ),
                };
                (
                    PosCommandInput::SaleCustomer(SaleCustomerInput {
                        id: id.clone(),
                        expected_version,
                        party_id: party_id.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("partyId", optional_json(party_id.as_deref())),
                    ]),
                )
            }
            PosOperation::SaleMoveTable => {
                object(
                    value,
                    &["id", "expectedVersion", "tableId"],
                    &["id", "expectedVersion", "tableId"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let table_id = text(value, "tableId")?;
                (
                    PosCommandInput::SaleMoveTable(SaleMoveTableInput {
                        id: id.clone(),
                        expected_version,
                        table_id: table_id.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("tableId", json_string(&table_id)),
                    ]),
                )
            }
            PosOperation::SaleRemove => {
                object(
                    value,
                    &["id", "expectedVersion", "lineId"],
                    &["id", "expectedVersion", "lineId"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let line_id = text(value, "lineId")?;
                (
                    PosCommandInput::SaleRemove(SaleRemoveInput {
                        id: id.clone(),
                        expected_version,
                        line_id: line_id.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("lineId", json_string(&line_id)),
                    ]),
                )
            }
            PosOperation::SaleDiscount => {
                object(
                    value,
                    &["id", "expectedVersion", "amountMinor"],
                    &["id", "expectedVersion", "amountMinor"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let amount_minor = money(integer(value, "amountMinor")?)?;
                (
                    PosCommandInput::SaleDiscount(SaleDiscountInput {
                        id: id.clone(),
                        expected_version,
                        amount_minor,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("amountMinor", amount_minor.to_string()),
                    ]),
                )
            }
            PosOperation::SaleMerge => {
                object(
                    value,
                    &["id", "expectedVersion", "targetId", "targetVersion"],
                    &["id", "expectedVersion", "targetId", "targetVersion"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let target_id = text(value, "targetId")?;
                let target_version = version(integer(value, "targetVersion")?)?;
                (
                    PosCommandInput::SaleMerge(SaleMergeInput {
                        id: id.clone(),
                        expected_version,
                        target_id: target_id.clone(),
                        target_version,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("targetId", json_string(&target_id)),
                        ("targetVersion", target_version.to_string()),
                    ]),
                )
            }
            PosOperation::SaleSplit => {
                object(
                    value,
                    &["id", "expectedVersion", "lineId", "quantityMilli"],
                    &["id", "expectedVersion", "lineId", "quantityMilli"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let line_id = text(value, "lineId")?;
                let quantity_milli = integer(value, "quantityMilli")?;
                if !(1..=999_999_999).contains(&quantity_milli) {
                    return Err(request_invalid("POS_SPLIT_INVALID", "Review the split quantity."));
                }
                (
                    PosCommandInput::SaleSplit(SaleSplitInput {
                        id: id.clone(),
                        expected_version,
                        line_id: line_id.clone(),
                        quantity_milli,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("lineId", json_string(&line_id)),
                        ("quantityMilli", quantity_milli.to_string()),
                    ]),
                )
            }
            PosOperation::PaymentCreate => {
                object(
                    value,
                    &["id", "expectedVersion", "method", "amountMinor", "tenderedMinor", "reference", "payerEmail"],
                    &["id", "expectedVersion", "method", "amountMinor", "tenderedMinor"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let method = text(value, "method")?;
                if !["cash", "transfer", "edc", "stripe_card", "stripe_promptpay"].contains(&method.as_str()) {
                    return Err(request_invalid("POS_COMMAND_INVALID", "Review the payment method."));
                }
                let amount_minor = money(integer(value, "amountMinor")?)?;
                if amount_minor == 0 {
                    return Err(request_invalid("POS_PAYMENT_AMOUNT", "Review the payment amount."));
                }
                let tendered_minor = money(integer(value, "tenderedMinor")?)?;
                let reference = value
                    .get("reference")
                    .and_then(|value| value.as_str())
                    .filter(|value| value.len() <= 150)
                    .unwrap_or("")
                    .to_owned();
                (
                    PosCommandInput::PaymentCreate(PaymentCreateInput {
                        id: id.clone(),
                        expected_version,
                        method: method.clone(),
                        amount_minor,
                        tendered_minor,
                        reference: reference.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("method", json_string(&method)),
                        ("amountMinor", amount_minor.to_string()),
                        ("tenderedMinor", tendered_minor.to_string()),
                        ("reference", json_string(&reference)),
                    ]),
                )
            }
            PosOperation::PaymentVerifyTransfer => {
                object(
                    value,
                    &["id", "expectedVersion", "bankTransactionId", "bankVersion"],
                    &["id", "expectedVersion", "bankTransactionId", "bankVersion"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let bank_transaction_id = text(value, "bankTransactionId")?;
                let bank_version = version(integer(value, "bankVersion")?)?;
                (
                    PosCommandInput::PaymentVerifyTransfer(PaymentVerifyTransferInput {
                        id: id.clone(),
                        expected_version,
                        bank_transaction_id: bank_transaction_id.clone(),
                        bank_version,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("bankTransactionId", json_string(&bank_transaction_id)),
                        ("bankVersion", bank_version.to_string()),
                    ]),
                )
            }
            PosOperation::PaymentCancelTransfer => {
                object(
                    value,
                    &["id", "expectedVersion"],
                    &["id", "expectedVersion"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                (
                    PosCommandInput::PaymentCancelTransfer(PaymentCancelTransferInput {
                        id: id.clone(),
                        expected_version,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                    ]),
                )
            }
            PosOperation::SaleComplete => {
                object(
                    value,
                    &["id", "expectedVersion"],
                    &["id", "expectedVersion"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                (
                    PosCommandInput::SaleComplete(SaleVersionInput {
                        id: id.clone(),
                        expected_version,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                    ]),
                )
            }
            PosOperation::SaleHold | PosOperation::SaleResume | PosOperation::SaleCancel => {
                object(
                    value,
                    &["id", "expectedVersion", "attachmentSetIds"],
                    &["id", "expectedVersion"],
                )?;
                attachments(value)?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                (
                    PosCommandInput::SaleVersion(SaleVersionInput {
                        id: id.clone(),
                        expected_version,
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                    ]),
                )
            }
            PosOperation::RefundApprove => {
                object(
                    value,
                    &["id", "expectedVersion", "reason", "attachmentSetIds"],
                    &["id", "expectedVersion", "reason"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let reason = reason(&text(value, "reason")?)?;
                (
                    PosCommandInput::RefundApprove(RefundApproveInput {
                        id: id.clone(),
                        expected_version,
                        reason: reason.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("reason", json_string(&reason)),
                    ]),
                )
            }
            PosOperation::RefundRequest => {
                object(
                    value,
                    &[
                        "id",
                        "expectedVersion",
                        "reason",
                        "lines",
                        "amountMinor",
                        "payoutShiftId",
                    ],
                    &["id", "expectedVersion", "reason", "lines"],
                )?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let reason = reason(&text(value, "reason")?)?;
                let lines = value
                    .get("lines")
                    .and_then(serde_json::Value::as_array)
                    .ok_or_else(|| {
                        request_invalid("POS_COMMAND_INVALID", "Review the refund lines.")
                    })?
                    .iter()
                    .map(|line| {
                        object(
                            line,
                            &["lineId", "quantityMilli", "restock"],
                            &["lineId", "quantityMilli", "restock"],
                        )?;
                        let line_id = text(line, "lineId")?;
                        let quantity_milli = integer(line, "quantityMilli")?;
                        if quantity_milli <= 0 || quantity_milli > 999_999_999 {
                            return Err(request_invalid(
                                "POS_REFUND_QUANTITY",
                                "Refund quantity exceeds the original item.",
                            ));
                        }
                        Ok(RefundLineInput {
                            line_id,
                            quantity_milli,
                            restock: boolean(line, "restock")?,
                        })
                    })
                    .collect::<Result<Vec<_>, BusinessError>>()?;
                if lines.len() > 100 {
                    return Err(request_invalid(
                        "POS_COMMAND_INVALID",
                        "Review the refund lines.",
                    ));
                }
                let amount_minor = match value.get("amountMinor") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(value) => Some(money(value.as_i64().ok_or_else(|| {
                        request_invalid("POS_COMMAND_INVALID", "Review the refund amount.")
                    })?)?),
                };
                let payout_shift_id = optional_text(value, "payoutShiftId")?;
                (
                    PosCommandInput::RefundRequest(RefundRequestInput {
                        id: id.clone(),
                        expected_version,
                        reason: reason.clone(),
                        lines: lines.clone(),
                        amount_minor,
                        payout_shift_id: payout_shift_id.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("reason", json_string(&reason)),
                        ("lines", lines_json(&lines)),
                        (
                            "amountMinor",
                            amount_minor.map_or_else(|| "null".into(), |value| value.to_string()),
                        ),
                        ("payoutShiftId", optional_json(payout_shift_id.as_deref())),
                    ]),
                )
            }
            PosOperation::RefundReject => {
                object(
                    value,
                    &["id", "expectedVersion", "reason", "attachmentSetIds"],
                    &["id", "expectedVersion", "reason"],
                )?;
                attachments(value)?;
                let id = text(value, "id")?;
                let expected_version = version(integer(value, "expectedVersion")?)?;
                let reason = reason(&text(value, "reason")?)?;
                (
                    PosCommandInput::RefundReject(RefundRejectInput {
                        id: id.clone(),
                        expected_version,
                        reason: reason.clone(),
                    }),
                    canonical(&[
                        ("id", json_string(&id)),
                        ("expectedVersion", expected_version.to_string()),
                        ("reason", json_string(&reason)),
                    ]),
                )
            }
        };
        Ok(Self {
            key,
            operation,
            input,
            canonical_input,
        })
    }
    pub fn receipt_identity(&self, actor: &PosActor) -> String {
        let branch = actor
            .branch_id
            .as_ref()
            .map_or_else(|| "null".to_owned(), |value| json_string(value.as_str()));
        format!(
            "{{\"user\":{},\"assignment\":{},\"company\":{},\"branch\":{},\"operation\":{},\"input\":{}}}",
            json_string(actor.user_id.as_str()),
            json_string(actor.assignment_id.as_str()),
            json_string(actor.company_id.as_str()),
            branch,
            json_string(self.operation()),
            self.canonical_input,
        )
    }
}

pub fn parse_receipt_result(value: &str) -> Result<PosResult, BusinessError> {
    if value.len() > 4_096 {
        return Err(source_invalid());
    }
    let parsed: serde_json::Value = serde_json::from_str(value).map_err(|_| source_invalid())?;
    let object = parsed.as_object().ok_or_else(source_invalid)?;
    if !(1..=2).contains(&object.len()) || !object.contains_key("id") {
        return Err(source_invalid());
    }
    if object.len() == 2 && !object.contains_key("version") {
        return Err(source_invalid());
    }
    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(source_invalid)?;
    if id.is_empty() || id.len() > 200 {
        return Err(source_invalid());
    }
    let version = match object.get("version") {
        None => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|value| *value >= 1)
                .ok_or_else(source_invalid)?,
        ),
    };
    Ok(PosResult {
        id: id.to_owned(),
        version,
    })
}
