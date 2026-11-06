//! Native POS register-session, bill, void and refund command ordering.
#![allow(clippy::field_reassign_with_default)]
use erpx_server_pos::public::commands::{
    CashApproveInput, PosActor, PosCashMovement, PosCommand, PosCommandRepository, PosFinancePort,
    PosFulfillment, PosFulfillmentInsert, PosFuture, PosHospitalityPort, PosJournalRequest, PosLine,
    PosConsumptionPart, PosFulfillmentSplitFacts, PosFulfillmentStock, PosInventoryPort,
    PosInvoiceRequest, PosLineDetail, PosLineDiscountRow, PosLineInsert, PosLineSplitRow, PosOperation,
    PosOrganizationPort, PosPaymentInsert, PosReceipt, PosReceiptRequest, PosRefund, PosStockRow,
    PosRegister, PosResult, PosSale, PosSettings, PosShift, execute_pos_command,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis, UserId,
};

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);
const ID: &str = "0f8fad5b-d9cb-469f-a165-70867728950e";

fn actor() -> PosActor {
    PosActor {
        user_id: UserId::new("manager").unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: RecordId::new("branch").ok(),
        department_id: None,
        assignment_id: AssignmentId::new("assignment").unwrap(),
        permissions: vec!["pos.sell".into(), "pos.approve".into()],
        modules: vec!["pos".into()],
        expires_at: NOW.get() + 60_000,
    }
}
fn settings() -> PosSettings {
    PosSettings {
        enabled: true,
        currency: "THB".into(),
        cashier_discount_bps: 0,
        accounts_json:
            r#"{"chartId":"chart","cash":"cash","cashTransfer":"transfer","cashDifference":"difference","edc":"edc","receivable":"receivable","advance":"advance","revenue":"revenue","outputTax":"outputTax","cogs":"cogs","inventory":"inventory"}"#
                .into(),
        accounting_approved_by: Some("finance".into()),
    }
}
fn register() -> PosRegister {
    PosRegister {
        id: "register".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        warehouse_id: "warehouse".into(),
        name: "Register".into(),
        mode: "retail".into(),
        active: true,
        property_id: None,
    }
}
fn shift(state: &str, version: i64) -> PosShift {
    PosShift {
        id: "shift".into(),
        register_id: "register".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        business_date: "2026-11-06".into(),
        opened_by: "cashier".into(),
        opening_minor: 100_000,
        state: state.into(),
        counted_minor: None,
        expected_minor: None,
        variance_minor: None,
        closed_by: None,
        version,
    }
}
fn sale(state: &str, version: i64) -> PosSale {
    PosSale {
        id: "sale".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        register_id: "register".into(),
        shift_id: "shift".into(),
        number: "POS-1".into(),
        party_id: None,
        issuer_json: "{}".into(),
        currency: "THB".into(),
        business_date: "2026-11-06".into(),
        service_mode: "retail".into(),
        state: state.into(),
        subtotal_minor: 10_000,
        tax_minor: 700,
        discount_minor: 0,
        total_minor: 10_700,
        created_by: "cashier".into(),
        version,
        accounts_json: settings().accounts_json,
        discount_approved_by: None,
        invoice_id: None,
    }
}
fn line(id: &str, quantity: i64, subtotal: i64, tax: i64) -> PosLine {
    PosLine {
        id: id.into(),
        sale_id: "sale".into(),
        quantity_milli: quantity,
        subtotal_minor: subtotal,
        tax_minor: tax,
        returned_milli: 0,
        price_minor: 10_000,
    }
}

struct Probe {
    calls: Vec<&'static str>,
    hash: String,
    receipt: Option<PosReceipt>,
    settings: Option<PosSettings>,
    register: Option<PosRegister>,
    shift: Option<PosShift>,
    open_exists: bool,
    day: bool,
    open_sales: bool,
    unapproved: bool,
    cash_sales: i64,
    cash_refunds: i64,
    approved: i64,
    close: bool,
    movement: Option<PosCashMovement>,
    approve: bool,
    bump: bool,
    review: bool,
    sale: Option<PosSale>,
    lines: Vec<PosLine>,
    payment_started: bool,
    set_state: bool,
    fulfillments: Vec<PosFulfillment>,
    shared: bool,
    cancel_sale: bool,
    table: bool,
    prepared: bool,
    refund: Option<PosRefund>,
    reject: bool,
    receipt_insert: bool,
    postings: Vec<String>,
    audits: usize,
    aliases: Vec<(String, String)>,
}
impl Default for Probe {
    fn default() -> Self {
        Self {
            hash: "a".repeat(64),
            settings: Some(settings()),
            register: Some(register()),
            shift: Some(shift("open", 1)),
            day: true,
            close: true,
            approve: true,
            bump: true,
            review: true,
            sale: Some(sale("open", 1)),
            lines: vec![line("line", 2_000, 10_000, 700)],
            set_state: true,
            cancel_sale: true,
            table: true,
            receipt_insert: true,
            calls: Vec::new(),
            receipt: None,
            open_exists: false,
            open_sales: false,
            unapproved: false,
            cash_sales: 0,
            cash_refunds: 0,
            approved: 0,
            movement: None,
            payment_started: false,
            fulfillments: Vec::new(),
            shared: false,
            prepared: false,
            refund: None,
            reject: true,
            postings: Vec::new(),
            audits: 0,
            aliases: Vec::new(),
        }
    }
}
impl PosCommandRepository for Probe {
    type Error = BusinessError;
    fn digest<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, String> {
        self.calls.push("digest");
        Box::pin(async move { Ok(self.hash.clone()) })
    }
    fn receipt<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, Option<PosReceipt>> {
        self.calls.push("receipt");
        Box::pin(async move { Ok(self.receipt.clone()) })
    }
    fn insert_receipt<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: TimestampMillis,
    ) -> PosFuture<'a, bool> {
        self.calls.push("receipt-insert");
        Box::pin(async move { Ok(self.receipt_insert) })
    }
    fn audit<'a>(
        &'a mut self,
        _: &'a PosActor,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: TimestampMillis,
    ) -> PosFuture<'a, ()> {
        self.calls.push("audit");
        self.audits += 1;
        Box::pin(async { Ok(()) })
    }
    fn new_id(&mut self) -> PosFuture<'_, String> {
        self.calls.push("new-id");
        Box::pin(async { Ok(ID.into()) })
    }
    fn business_date(&mut self, _: TimestampMillis) -> PosFuture<'_, String> {
        self.calls.push("business-date");
        Box::pin(async { Ok("2026-11-06".into()) })
    }
    fn same_person<'a>(
        &'a mut self,
        _: &'a CompanyId,
        left: &'a str,
        right: &'a str,
    ) -> PosFuture<'a, bool> {
        self.calls.push("same-person");
        let same = left == right
            || self
                .aliases
                .iter()
                .any(|(a, b)| (a == left && b == right) || (a == right && b == left));
        Box::pin(async move { Ok(same) })
    }
    fn settings<'a>(&'a mut self, _: &'a CompanyId) -> PosFuture<'a, Option<PosSettings>> {
        self.calls.push("settings");
        Box::pin(async move { Ok(self.settings.clone()) })
    }
    fn register<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> PosFuture<'a, Option<PosRegister>> {
        self.calls.push("register");
        Box::pin(async move { Ok(self.register.clone()) })
    }
    fn shift<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, Option<PosShift>> {
        self.calls.push("shift");
        Box::pin(async move { Ok(self.shift.clone()) })
    }
    fn shift_open_exists<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        self.calls.push("shift-open");
        Box::pin(async move { Ok(self.open_exists) })
    }
    fn day_writable<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, bool> {
        self.calls.push("day");
        Box::pin(async move { Ok(self.day) })
    }
    fn insert_shift<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a PosRegister,
        _: &'a PosActor,
        _: i64,
        _: &'a str,
        _: TimestampMillis,
    ) -> PosFuture<'a, ()> {
        self.calls.push("insert-shift");
        Box::pin(async { Ok(()) })
    }
    fn open_sales<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        self.calls.push("open-sales");
        Box::pin(async move { Ok(self.open_sales) })
    }
    fn unapproved_cash<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        self.calls.push("unapproved");
        Box::pin(async move { Ok(self.unapproved) })
    }
    fn cash_sales<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, i64> {
        self.calls.push("cash-sales");
        Box::pin(async move { Ok(self.cash_sales) })
    }
    fn cash_refunds<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, i64> {
        self.calls.push("cash-refunds");
        Box::pin(async move { Ok(self.cash_refunds) })
    }
    fn approved_movements<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, i64> {
        self.calls.push("approved-movements");
        Box::pin(async move { Ok(self.approved) })
    }
    fn close_shift<'a>(
        &'a mut self,
        _: &'a str,
        _: i64,
        _: i64,
        _: i64,
        _: i64,
        _: Option<&'a str>,
        _: &'a PosActor,
        _: TimestampMillis,
    ) -> PosFuture<'a, bool> {
        self.calls.push("close-shift");
        Box::pin(async move { Ok(self.close) })
    }
    fn cash_movement<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, Option<PosCashMovement>> {
        self.calls.push("movement");
        Box::pin(async move { Ok(self.movement.clone()) })
    }
    fn approve_cash<'a>(&'a mut self, _: &'a str, _: i64, _: &'a PosActor) -> PosFuture<'a, bool> {
        self.calls.push("approve-cash");
        Box::pin(async move { Ok(self.approve) })
    }
    fn bump_shift<'a>(&'a mut self, _: &'a str, _: i64) -> PosFuture<'a, bool> {
        self.calls.push("bump-shift");
        Box::pin(async move { Ok(self.bump) })
    }
    fn review_shift<'a>(
        &'a mut self,
        _: &'a str,
        _: i64,
        _: &'a PosActor,
        _: &'a str,
    ) -> PosFuture<'a, bool> {
        self.calls.push("review-shift");
        Box::pin(async move { Ok(self.review) })
    }
    fn insert_posting<'a>(
        &'a mut self,
        key: &'a str,
        _: &'a CompanyId,
        journal: &'a str,
    ) -> PosFuture<'a, ()> {
        self.calls.push("posting");
        self.postings.push(format!("{key}:{journal}"));
        Box::pin(async { Ok(()) })
    }
    fn sale<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, Option<PosSale>> {
        self.calls.push("sale");
        Box::pin(async move { Ok(self.sale.clone()) })
    }
    fn sale_lines<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, Vec<PosLine>> {
        self.calls.push("lines");
        Box::pin(async move { Ok(self.lines.clone()) })
    }
    fn payment_started<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        self.calls.push("payment-started");
        Box::pin(async move { Ok(self.payment_started) })
    }
    fn set_sale_state<'a>(
        &'a mut self,
        _: &'a str,
        _: i64,
        _: &'a str,
        _: TimestampMillis,
    ) -> PosFuture<'a, bool> {
        self.calls.push("set-state");
        Box::pin(async move { Ok(self.set_state) })
    }
    fn sale_fulfillments<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, Vec<PosFulfillment>> {
        self.calls.push("fulfillments");
        Box::pin(async move { Ok(self.fulfillments.clone()) })
    }
    fn fulfillment_shared<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, bool> {
        self.calls.push("shared");
        Box::pin(async move { Ok(self.shared) })
    }
    fn cancel_fulfillment<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, ()> {
        self.calls.push("cancel-fulfillment");
        Box::pin(async { Ok(()) })
    }
    fn cancel_sale<'a>(
        &'a mut self,
        _: &'a str,
        _: i64,
        _: TimestampMillis,
    ) -> PosFuture<'a, bool> {
        self.calls.push("cancel-sale");
        Box::pin(async move { Ok(self.cancel_sale) })
    }
    fn table_exists<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, bool> {
        self.calls.push("table");
        Box::pin(async move { Ok(self.table) })
    }
    fn insert_sale<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a PosShift,
        _: &'a PosRegister,
        _: &'a PosSettings,
        _: &'a erpx_server_pos::public::commands::SaleCreateInput,
        _: &'a str,
        _: &'a str,
        _: &'a PosActor,
        _: &'a str,
        _: TimestampMillis,
    ) -> PosFuture<'a, ()> {
        self.calls.push("insert-sale");
        Box::pin(async { Ok(()) })
    }
    fn prepared_day_close<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, bool> {
        self.calls.push("prepared-day");
        Box::pin(async move { Ok(self.prepared) })
    }
    fn insert_refund<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a PosSale,
        _: &'a erpx_server_pos::public::commands::RefundRequestInput,
        _: &'a str,
        _: &'a str,
        _: i64,
        _: i64,
        _: &'a str,
        _: &'a PosActor,
        _: TimestampMillis,
    ) -> PosFuture<'a, ()> {
        self.calls.push("insert-refund");
        Box::pin(async { Ok(()) })
    }
    fn refund<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, Option<PosRefund>> {
        self.calls.push("refund");
        Box::pin(async move { Ok(self.refund.clone()) })
    }
    fn reject_refund<'a>(&'a mut self, _: &'a str, _: i64, _: &'a PosActor) -> PosFuture<'a, bool> {
        self.calls.push("reject-refund");
        Box::pin(async move { Ok(self.reject) })
    }

    fn sale_paid_total<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, i64> {
        Box::pin(async { Ok(0) })
    }
    fn sale_pending_payments<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async { Ok(false) })
    }
    fn delete_reservation<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    #[allow(clippy::too_many_arguments)]
    fn insert_stock_move<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: i64,
        _: i64,
        _: &'a str,
        _: &'a str,
        _: TimestampMillis,
    ) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn mark_fulfillment_delivered<'a>(&'a mut self, _: &'a str, _: i64) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn complete_sale<'a>(&'a mut self, _: &'a str, _: TimestampMillis) -> PosFuture<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn attach_invoice<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn payment_reserved<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, i64> {
        Box::pin(async { Ok(0) })
    }
    fn sale_gross<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, i64> {
        Box::pin(async { Ok(0) })
    }
    fn sale_fulfillment_stock<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, Vec<PosFulfillmentStock>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn insert_payment<'a>(&'a mut self, _: &'a PosPaymentInsert) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn settle_payment<'a>(&'a mut self, _: &'a str, _: TimestampMillis) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn attach_finance_payment<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn reserve_stock<'a>(&'a mut self, _: &'a str, _: &'a str, _: &'a str, _: i64) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn mark_fulfillment_reserved<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn bump_payment_pending<'a>(&'a mut self, _: &'a str, _: TimestampMillis) -> PosFuture<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn sale_line_split<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, Option<PosLineSplitRow>> {
        Box::pin(async { Ok(None) })
    }
    fn fulfillment_split<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, Option<PosFulfillmentSplitFacts>> {
        Box::pin(async { Ok(None) })
    }
    fn create_split_sale<'a>(&'a mut self, _: &'a str, _: &'a str, _: &'a str, _: &'a str, _: TimestampMillis) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn move_line_to_sale<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn create_split_fulfillment<'a>(&'a mut self, _: &'a str, _: &'a str, _: i64, _: TimestampMillis) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn reduce_fulfillment<'a>(&'a mut self, _: &'a str, _: i64) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn create_split_line<'a>(&'a mut self, _: &'a str, _: &'a str, _: &'a str, _: &'a str, _: i64, _: i64, _: i64, _: i64, _: i64) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn reduce_line<'a>(&'a mut self, _: &'a str, _: i64, _: i64, _: i64, _: i64, _: i64) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn move_lines<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn mark_merged<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn sale_lines_discount<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, Vec<PosLineDiscountRow>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn update_line_discount<'a>(&'a mut self, _: &'a str, _: i64, _: i64, _: i64, _: i64) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn set_sale_party<'a>(&'a mut self, _: &'a str, _: Option<&'a str>) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn set_sale_table<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn delete_reservations<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn delete_line<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn insert_fulfillment<'a>(&'a mut self, _: &'a PosFulfillmentInsert) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn insert_line<'a>(&'a mut self, _: &'a PosLineInsert) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn refresh_sale<'a>(&'a mut self, _: &'a str, _: TimestampMillis) -> PosFuture<'a, bool> {
        Box::pin(async { Ok(true) })
    }
    fn sale_line<'a>(&'a mut self, _: &'a str, _: &'a str) -> PosFuture<'a, Option<PosLineDetail>> {
        Box::pin(async { Ok(None) })
    }
    fn update_fulfillment<'a>(&'a mut self, _: &'a str, _: i64, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
    fn update_line<'a>(&'a mut self, _: &'a str, _: i64, _: i64, _: i64, _: i64, _: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Default)]
struct Finance {
    journal: Option<String>,
    party: bool,
    calls: Vec<String>,
}
impl PosFinancePort for Finance {
    fn open_period<'a>(&'a mut self, _: &'a CompanyId, date: &'a str) -> PosFuture<'a, ()> {
        self.calls.push(format!("period:{date}"));
        Box::pin(async { Ok(()) })
    }
    fn post_journal<'a>(
        &'a mut self,
        request: &'a PosJournalRequest,
    ) -> PosFuture<'a, Option<String>> {
        self.calls.push(format!(
            "journal:{}:{}:{}",
            request.source_key, request.lines[0].debit, request.lines[1].credit
        ));
        Box::pin(async move { Ok(self.journal.clone()) })
    }
    fn party_exists<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, bool> {
        self.calls.push("party".into());
        Box::pin(async move { Ok(self.party) })
    }
    fn record_receipt<'a>(&'a mut self, request: &'a PosReceiptRequest) -> PosFuture<'a, ()> {
        self.calls.push(format!("receipt:{}", request.amount_minor));
        Box::pin(async { Ok(()) })
    }
    fn record_invoice<'a>(&'a mut self, request: &'a PosInvoiceRequest) -> PosFuture<'a, ()> {
        self.calls.push(format!("invoice:{}", request.total_minor));
        Box::pin(async { Ok(()) })
    }
}
#[derive(Default)]
struct Inventory {
    calls: Vec<String>,
}
impl PosInventoryPort for Inventory {
    fn queue_consumption<'a>(
        &'a mut self,
        parts: &'a [PosConsumptionPart],
    ) -> PosFuture<'a, i64> {
        self.calls.push(format!("consume:{}", parts.len()));
        Box::pin(async move { Ok(parts.iter().map(|part| part.expected_value_minor).sum()) })
    }
}
#[derive(Default)]
struct Hospitality {
    hotel: bool,
    room: bool,
}
impl PosHospitalityPort for Hospitality {
    fn hotel_day_open<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, bool> {
        Box::pin(async move { Ok(self.hotel) })
    }
    fn room_charge_exists<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async move { Ok(self.room) })
    }
}
struct Organization;
impl PosOrganizationPort for Organization {
    fn issuer_name<'a>(&'a mut self, _: &'a CompanyId) -> PosFuture<'a, String> {
        Box::pin(async { Ok("Company".into()) })
    }
}

fn command(operation: PosOperation, value: serde_json::Value) -> PosCommand {
    PosCommand::parse(
        operation,
        CommandKey::new("pos-command-0001").unwrap(),
        &value,
    )
    .unwrap()
}
fn run(
    probe: &mut Probe,
    finance: &mut Finance,
    hospitality: &mut Hospitality,
    command: &PosCommand,
) -> Result<PosResult, BusinessError> {
    sqlx::test_block_on(execute_pos_command(
        &actor(),
        command,
        probe,
        None,
        None,
        finance,
        &mut Inventory::default(),
        hospitality,
        &mut Organization,
        NOW,
    ))
}

#[test]
fn permission_module_expiry_and_department_precede_every_read() {
    for mutate in [
        |actor: &mut PosActor| actor.permissions.clear(),
        |actor: &mut PosActor| actor.modules.clear(),
        |actor: &mut PosActor| actor.expires_at = NOW.get(),
        |actor: &mut PosActor| actor.department_id = RecordId::new("department").ok(),
    ] {
        let mut actor = actor();
        mutate(&mut actor);
        let mut probe = Probe::default();
        let error = sqlx::test_block_on(execute_pos_command(
            &actor,
            &command(
                PosOperation::ShiftOpen,
                serde_json::json!({"registerId":"register","openingMinor":1000}),
            ),
            &mut probe,
            None,
            None,
            &mut Finance::default(),
            &mut Inventory::default(),
            &mut Hospitality::default(),
            &mut Organization,
            NOW,
        ))
        .unwrap_err();
        assert!(matches!(
            error.kind,
            FailureKind::Forbidden | FailureKind::Conflict
        ));
        assert!(probe.calls.is_empty());
    }
}

#[test]
fn shift_open_checks_setup_day_finance_period_and_register_scope() {
    let mut probe = Probe::default();
    let mut finance = Finance::default();
    let result = run(
        &mut probe,
        &mut finance,
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftOpen,
            serde_json::json!({"registerId":"register","openingMinor":1000}),
        ),
    )
    .unwrap();
    assert_eq!(result.id, ID);
    assert_eq!(finance.calls, vec!["period:2026-11-06"]);
    assert!(probe.calls.contains(&"insert-shift"));
    assert_eq!(probe.audits, 1);

    let mut no_setup = Probe::default();
    no_setup.settings = None;
    let error = run(
        &mut no_setup,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftOpen,
            serde_json::json!({"registerId":"register","openingMinor":1000}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_SETUP_REQUIRED");
    assert!(!no_setup.calls.contains(&"insert-shift"));

    let mut closed = Probe::default();
    closed.day = false;
    let error = run(
        &mut closed,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftOpen,
            serde_json::json!({"registerId":"register","openingMinor":1000}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_DAY_CLOSED");
    assert!(!closed.calls.contains(&"insert-shift"));

    let mut existing = Probe::default();
    existing.open_exists = true;
    let error = run(
        &mut existing,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftOpen,
            serde_json::json!({"registerId":"register","openingMinor":1000}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_REGISTER_OPEN");
    assert!(!existing.calls.contains(&"insert-shift"));
}

#[test]
fn shift_close_uses_opening_sales_refunds_and_approved_movements_and_cas() {
    let mut probe = Probe::default();
    probe.cash_sales = 40_000;
    probe.cash_refunds = 5_000;
    probe.approved = 1_000;
    let result = run(
        &mut probe,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftClose,
            serde_json::json!({"id":"shift","expectedVersion":1,"countedMinor":136500}),
        ),
    )
    .unwrap();
    assert_eq!(result.version, Some(2));
    assert_eq!(
        probe.calls,
        vec![
            "digest",
            "receipt",
            "shift",
            "open-sales",
            "unapproved",
            "cash-sales",
            "cash-refunds",
            "approved-movements",
            "close-shift",
            "receipt-insert",
            "audit"
        ]
    );

    for (field, code) in [
        ("open-sales", "POS_OPEN_SALES"),
        ("unapproved", "POS_CASH_REVIEW_PENDING"),
    ] {
        let mut probe = Probe::default();
        if field == "open-sales" {
            probe.open_sales = true;
        } else {
            probe.unapproved = true;
        }
        let error = run(
            &mut probe,
            &mut Finance::default(),
            &mut Hospitality::default(),
            &command(
                PosOperation::ShiftClose,
                serde_json::json!({"id":"shift","expectedVersion":1,"countedMinor":136500}),
            ),
        )
        .unwrap_err();
        assert_eq!(error.code, code);
        assert!(!probe.calls.contains(&"close-shift"));
    }

    let mut probe = Probe::default();
    probe.close = false;
    let error = run(
        &mut probe,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftClose,
            serde_json::json!({"id":"shift","expectedVersion":1,"countedMinor":136500}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_VERSION_CONFLICT");
}

#[test]
fn shift_review_requires_an_independent_reviewer_and_posts_only_the_variance() {
    let mut probe = Probe::default();
    probe.shift = Some(PosShift {
        state: "closed".into(),
        counted_minor: Some(101_000),
        expected_minor: Some(100_000),
        variance_minor: Some(1_000),
        closed_by: Some("cashier".into()),
        version: 2,
        ..shift("closed", 2)
    });
    let mut finance = Finance::default();
    finance.journal = Some("journal-shift".into());
    let result = run(
        &mut probe,
        &mut finance,
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftReview,
            serde_json::json!({"id":"shift","expectedVersion":2,"reason":"Count reviewed","attachmentSetIds":[]}),
        ),
    )
    .unwrap();
    assert_eq!(result.id, "shift");
    assert_eq!(finance.calls, vec!["journal:shift:shift:1000:1000"]);
    assert_eq!(probe.postings, vec!["shift:shift:journal-shift"]);

    let mut same = Probe::default();
    same.shift = Some(PosShift {
        state: "closed".into(),
        variance_minor: Some(0),
        closed_by: Some("manager".into()),
        version: 2,
        ..shift("closed", 2)
    });
    let error = run(
        &mut same,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftReview,
            serde_json::json!({"id":"shift","expectedVersion":2,"reason":"Count reviewed"}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_INDEPENDENT_REVIEW");
    assert!(!same.calls.contains(&"review-shift"));

    let mut zero = Probe::default();
    zero.shift = Some(PosShift {
        state: "closed".into(),
        variance_minor: Some(0),
        closed_by: Some("cashier".into()),
        version: 2,
        ..shift("closed", 2)
    });
    let mut finance = Finance::default();
    run(
        &mut zero,
        &mut finance,
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftReview,
            serde_json::json!({"id":"shift","expectedVersion":2,"reason":"Count reviewed"}),
        ),
    )
    .unwrap();
    assert!(finance.calls.is_empty());
    assert!(zero.postings.is_empty());
}

#[test]
fn cash_approval_requires_an_independent_actor_posts_the_pair_and_bumps_the_shift() {
    let mut probe = Probe::default();
    probe.movement = Some(PosCashMovement {
        id: "movement".into(),
        shift_id: "shift".into(),
        amount_minor: -5_000,
        created_by: "cashier".into(),
        approved_by: None,
    });
    let mut finance = Finance::default();
    finance.journal = Some("journal-cash".into());
    run(
        &mut probe,
        &mut finance,
        &mut Hospitality::default(),
        &command(
            PosOperation::CashApprove,
            serde_json::json!({"id":"shift","expectedVersion":1,"movementId":"movement"}),
        ),
    )
    .unwrap();
    assert_eq!(finance.calls, vec!["journal:cash:movement:5000:5000"]);
    assert_eq!(probe.postings, vec!["cash:movement:journal-cash"]);
    assert!(probe.calls.contains(&"approve-cash"));
    assert!(probe.calls.contains(&"bump-shift"));

    let mut same = Probe::default();
    same.movement = Some(PosCashMovement {
        id: "movement".into(),
        shift_id: "shift".into(),
        amount_minor: 5_000,
        created_by: "manager".into(),
        approved_by: None,
    });
    let error = run(
        &mut same,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::CashApprove,
            serde_json::json!({"id":"shift","expectedVersion":1,"movementId":"movement"}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_INDEPENDENT_REVIEW");
    assert!(!same.calls.contains(&"approve-cash"));
}

#[test]
fn sale_create_checks_settings_shift_date_register_mode_table_and_customer() {
    let mut probe = Probe::default();
    let mut finance = Finance::default();
    finance.party = true;
    let result = run(
        &mut probe,
        &mut finance,
        &mut Hospitality::default(),
        &command(
            PosOperation::SaleCreate,
            serde_json::json!({"shiftId":"shift","tableId":"table","guests":2,"partyId":"customer","serviceMode":"retail","currency":"THB"}),
        ),
    )
    .unwrap();
    assert_eq!(result.id, ID);
    assert!(probe.calls.contains(&"insert-sale"));
    assert_eq!(finance.calls, vec!["party"]);

    let mut foreign_party = Probe::default();
    let error = run(
        &mut foreign_party,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::SaleCreate,
            serde_json::json!({"shiftId":"shift","tableId":null,"guests":1,"partyId":"customer","serviceMode":"retail"}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_CUSTOMER_INVALID");
    assert!(!foreign_party.calls.contains(&"insert-sale"));

    let mut mode = Probe::default();
    mode.register = Some(PosRegister {
        mode: "restaurant".into(),
        ..register()
    });
    let error = run(
        &mut mode,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::SaleCreate,
            serde_json::json!({"shiftId":"shift","tableId":null,"guests":1,"partyId":null,"serviceMode":"retail"}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_MODE_INVALID");
}

#[test]
fn unpaid_void_releases_only_draft_kitchen_work_and_checks_payments_and_sharing() {
    let mut probe = Probe::default();
    probe.fulfillments = vec![
        PosFulfillment {
            id: "draft".into(),
            state: "draft".into(),
            kind: "stock".into(),
        },
        PosFulfillment {
            id: "queued".into(),
            state: "queued".into(),
            kind: "recipe".into(),
        },
    ];
    let result = run(
        &mut probe,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::SaleCancel,
            serde_json::json!({"id":"sale","expectedVersion":1,"attachmentSetIds":[]}),
        ),
    )
    .unwrap();
    assert_eq!(result.version, Some(2));
    assert_eq!(
        probe
            .calls
            .iter()
            .filter(|call| **call == "cancel-fulfillment")
            .count(),
        2
    );

    let mut paid = Probe::default();
    paid.payment_started = true;
    let error = run(
        &mut paid,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::SaleCancel,
            serde_json::json!({"id":"sale","expectedVersion":1}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_PAYMENT_STARTED");

    let mut started = Probe::default();
    started.fulfillments = vec![PosFulfillment {
        id: "prep".into(),
        state: "preparing".into(),
        kind: "recipe".into(),
    }];
    let error = run(
        &mut started,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::SaleCancel,
            serde_json::json!({"id":"sale","expectedVersion":1}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_KITCHEN_ACTIVE");

    let mut shared = Probe::default();
    shared.fulfillments = vec![PosFulfillment {
        id: "draft".into(),
        state: "draft".into(),
        kind: "stock".into(),
    }];
    shared.shared = true;
    let error = run(
        &mut shared,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::SaleCancel,
            serde_json::json!({"id":"sale","expectedVersion":1}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_SHARED_KITCHEN");
}

#[test]
fn refund_request_rejects_duplicates_and_over_quantities_and_computes_tax() {
    let mut probe = Probe::default();
    probe.sale = Some(sale("completed", 2));
    probe.lines = vec![line("line", 2_000, 10_000, 700)];
    let result = run(
        &mut probe,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::RefundRequest,
            serde_json::json!({"id":"sale","expectedVersion":2,"reason":"Return one unit","lines":[{"lineId":"line","quantityMilli":1000,"restock":true}],"amountMinor":null,"payoutShiftId":null}),
        ),
    )
    .unwrap();
    assert_eq!(result.id, ID);
    assert!(probe.calls.contains(&"insert-refund"));

    for (value, code) in [
        (
            serde_json::json!({"id":"sale","expectedVersion":2,"reason":"Return one unit","lines":[{"lineId":"line","quantityMilli":1000,"restock":true},{"lineId":"line","quantityMilli":1000,"restock":true}],"amountMinor":null,"payoutShiftId":null}),
            "POS_DUPLICATE_LINE",
        ),
        (
            serde_json::json!({"id":"sale","expectedVersion":2,"reason":"Return one unit","lines":[{"lineId":"line","quantityMilli":3000,"restock":true}],"amountMinor":null,"payoutShiftId":null}),
            "POS_REFUND_QUANTITY",
        ),
        (
            serde_json::json!({"id":"sale","expectedVersion":2,"reason":"Return one unit","lines":[],"amountMinor":null,"payoutShiftId":null}),
            "POS_REFUND_EMPTY",
        ),
    ] {
        let mut probe = Probe::default();
        probe.sale = Some(sale("completed", 2));
        probe.lines = vec![line("line", 2_000, 10_000, 700)];
        let error = run(
            &mut probe,
            &mut Finance::default(),
            &mut Hospitality::default(),
            &command(PosOperation::RefundRequest, value),
        )
        .unwrap_err();
        assert_eq!(error.code, code);
        assert!(!probe.calls.contains(&"insert-refund"));
    }

    let mut room = Probe::default();
    room.sale = Some(sale("completed", 2));
    let error = run(
        &mut room,
        &mut Finance::default(),
        &mut Hospitality { hotel: false, room: true },
        &command(
            PosOperation::RefundRequest,
            serde_json::json!({"id":"sale","expectedVersion":2,"reason":"Return one unit","lines":[{"lineId":"line","quantityMilli":1000,"restock":true}],"amountMinor":null,"payoutShiftId":null}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_ROOM_CHARGE_REVERSAL");
    assert!(!room.calls.contains(&"insert-refund"));
}

#[test]
fn merged_alias_blocks_self_review_through_the_identity_port() {
    let mut probe = Probe::default();
    probe.aliases = vec![("manager".into(), "cashier".into())];
    probe.shift = Some(PosShift {
        state: "closed".into(),
        variance_minor: Some(0),
        closed_by: Some("cashier".into()),
        version: 2,
        ..shift("closed", 2)
    });
    let error = run(
        &mut probe,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftReview,
            serde_json::json!({"id":"shift","expectedVersion":2,"reason":"Count reviewed"}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_INDEPENDENT_REVIEW");
    assert!(probe.calls.contains(&"same-person"));
    assert!(!probe.calls.contains(&"review-shift"));
}

#[test]
fn refund_reject_requires_an_independent_reviewer() {
    let mut probe = Probe::default();
    probe.refund = Some(PosRefund {
        id: "refund".into(),
        sale_id: "sale".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        state: "requested".into(),
        requested_by: "cashier".into(),
        version: 1,
    });
    run(
        &mut probe,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::RefundReject,
            serde_json::json!({"id":"refund","expectedVersion":1,"reason":"Evidence insufficient","attachmentSetIds":[]}),
        ),
    )
    .unwrap();
    assert!(probe.calls.contains(&"reject-refund"));

    let mut same = Probe::default();
    same.refund = Some(PosRefund {
        id: "refund".into(),
        sale_id: "sale".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        state: "requested".into(),
        requested_by: "manager".into(),
        version: 1,
    });
    let error = run(
        &mut same,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::RefundReject,
            serde_json::json!({"id":"refund","expectedVersion":1,"reason":"Evidence insufficient"}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "POS_INDEPENDENT_REVIEW");
    assert!(!same.calls.contains(&"reject-refund"));
}

#[test]
fn replay_returns_the_same_receipt_and_payload_mismatch_is_an_idempotency_conflict() {
    let mut probe = Probe::default();
    probe.receipt = Some(PosReceipt {
        request_hash: "a".repeat(64),
        result_json: format!(r#"{{"id":"{ID}"}}"#),
    });
    let result = run(
        &mut probe,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftOpen,
            serde_json::json!({"registerId":"register","openingMinor":1000}),
        ),
    )
    .unwrap();
    assert_eq!(result.id, ID);
    assert_eq!(probe.calls, vec!["digest", "receipt"]);

    let mut conflict = Probe::default();
    conflict.receipt = Some(PosReceipt {
        request_hash: "b".repeat(64),
        result_json: format!(r#"{{"id":"{ID}"}}"#),
    });
    let error = run(
        &mut conflict,
        &mut Finance::default(),
        &mut Hospitality::default(),
        &command(
            PosOperation::ShiftOpen,
            serde_json::json!({"registerId":"register","openingMinor":1000}),
        ),
    )
    .unwrap_err();
    assert_eq!(error.code, "IDEMPOTENCY_CONFLICT");
}

#[test]
fn cash_approval_input_is_exact_and_denominations_must_match_the_count() {
    assert!(PosCommand::parse(
        PosOperation::CashApprove,
        CommandKey::new("pos-command-0001").unwrap(),
        &serde_json::json!({"id":"shift","expectedVersion":1,"movementId":"movement","extra":1}),
    )
    .is_err());
    assert!(PosCommand::parse(
        PosOperation::ShiftClose,
        CommandKey::new("pos-command-0001").unwrap(),
        &serde_json::json!({"id":"shift","expectedVersion":1,"countedMinor":1000,"denominations":{"1000":1}}),
    )
    .is_ok());
    assert!(PosCommand::parse(
        PosOperation::ShiftClose,
        CommandKey::new("pos-command-0001").unwrap(),
        &serde_json::json!({"id":"shift","expectedVersion":1,"countedMinor":1000,"denominations":{"500":1}}),
    )
    .is_err());
    let _ = CashApproveInput {
        id: "shift".into(),
        expected_version: 1,
        movement_id: "movement".into(),
    };
}
