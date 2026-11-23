//! Authoritative SQL registry for every remaining POS owner write literal and the
//! retired POS owner read literals.
//!
//! This is the single owner model for the 79 TypeScript write statements retired in
//! the wf1a4 owner-wide slice plus the retired POS approval-queue read statements.
//! Each typed `kind` maps to one parameterized SQL string here; transport only
//! supplies positional values, never statement text. No TS copy and no fallback
//! writer/reader exists for a retired kind. The caller executes the returned
//! statement on the same serializable transaction that carries receipt/audit/outbox.

use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId, FailureKind};
use serde_json::Value;
use sqlx::{Connection, PgConnection};

type Error = PgTransactionError<BusinessError>;

/// An owner-owned write statement for the caller to execute on its unit.
#[derive(Clone, Debug)]
pub struct PosOwnerStatement {
    pub sql: String,
    pub values: Vec<Value>,
}

pub const CORE_SAVEREVERSALREQUEST_41: &str = "core_savereversalrequest_41";
pub const CORE_REVIEWREVERSALREQUEST_42: &str = "core_reviewreversalrequest_42";
pub const CORE_PREPAREACCOUNTING_43: &str = "core_prepareaccounting_43";
pub const CORE_LINKREGISTER_44: &str = "core_linkregister_44";
pub const CORE_WRITEPOSPOSTINGS1_63: &str = "core_writepospostings1_63";
pub const CORE_WRITEPOSAUDIT1_73: &str = "core_writeposaudit1_73";
pub const CORE_WRITEPOSJOBS1_81: &str = "core_writeposjobs1_81";
pub const CORE_WRITEPOSRESERVATIONS1_89: &str = "core_writeposreservations1_89";
pub const CORE_WRITEPOSFULFILLMENTS1_92: &str = "core_writeposfulfillments1_92";
pub const CORE_WRITEPOSRESERVATIONS2_97: &str = "core_writeposreservations2_97";
pub const CORE_WRITEPOSSTOCKMOVES1_112: &str = "core_writeposstockmoves1_112";
pub const CORE_WRITEPOSFULFILLMENTS2_116: &str = "core_writeposfulfillments2_116";
pub const CORE_WRITEPOSPAYMENTS1_125: &str = "core_writepospayments1_125";
pub const CORE_WRITEPOSSALES2_129: &str = "core_writepossales2_129";
pub const CORE_WRITEPOSFULFILLMENTS3_144: &str = "core_writeposfulfillments3_144";
pub const CORE_WRITEPOSSALES3_157: &str = "core_writepossales3_157";
pub const CORE_WRITEPOSRECEIPTS1_172: &str = "core_writeposreceipts1_172";
pub const CORE_WRITEPOSSETTINGS1_198: &str = "core_writepossettings1_198";
pub const CORE_WRITEPOSREGISTERS1_206: &str = "core_writeposregisters1_206";
pub const CORE_WRITEPOSOAUTHSTATES1_213: &str = "core_writeposoauthstates1_213";
pub const CORE_WRITEPOSSETTINGS2_227: &str = "core_writepossettings2_227";
pub const CORE_WRITEPOSREGISTERS2_242: &str = "core_writeposregisters2_242";
pub const CORE_WRITEPOSREGISTERS3_251: &str = "core_writeposregisters3_251";
pub const CORE_WRITEPOSCATALOG1_271: &str = "core_writeposcatalog1_271";
pub const CORE_WRITEPOSCATALOG2_298: &str = "core_writeposcatalog2_298";
pub const CORE_WRITEPOSREGISTERCATALOG1_303: &str = "core_writeposregistercatalog1_303";
pub const CORE_WRITEPOSREGISTERCATALOG2_306: &str = "core_writeposregistercatalog2_306";
pub const CORE_WRITEPOSREGISTERCATALOG3_310: &str = "core_writeposregistercatalog3_310";
pub const CORE_WRITEPOSSTOCKMOVES2_352: &str = "core_writeposstockmoves2_352";
pub const CORE_WRITEPOSROOMCHARGEREVERSALS1_365: &str = "core_writeposroomchargereversals1_365";
pub const CORE_WRITEPOSSALES6_369: &str = "core_writepossales6_369";
pub const CORE_WRITEPOSSALES7_389: &str = "core_writepossales7_389";
pub const CORE_WRITEPOSRESERVATIONS4_392: &str = "core_writeposreservations4_392";
pub const CORE_WRITEPOSFULFILLMENTS10_409: &str = "core_writeposfulfillments10_409";
pub const CORE_WRITEPOSPAYMENTS5_421: &str = "core_writepospayments5_421";
pub const CORE_WRITEPOSJOBS2_424: &str = "core_writeposjobs2_424";
pub const CORE_WRITEPOSSETTLEMENTS1_441: &str = "core_writepossettlements1_441";
pub const CORE_WRITEPOSSETTLEMENTS2_452: &str = "core_writepossettlements2_452";
pub const CORE_READPOSREFUNDPAYMENTS2_460: &str = "core_readposrefundpayments2_460";
pub const CORE_READPOSPAYMENTS14_464: &str = "core_readpospayments14_464";
pub const CORE_WRITEPOSJOBS3_468: &str = "core_writeposjobs3_468";
pub const CORE_WRITEPOSREFUNDS1_476: &str = "core_writeposrefunds1_476";
pub const CORE_READPOSREFUNDPAYMENTS4_479: &str = "core_readposrefundpayments4_479";
pub const CORE_WRITEPOSJOBS4_483: &str = "core_writeposjobs4_483";
pub const CORE_WRITEPOSREFUNDPAYMENTS2_499: &str = "core_writeposrefundpayments2_499";
pub const CORE_WRITEPOSREFUNDS5_503: &str = "core_writeposrefunds5_503";
pub const CORE_WRITEPOSRESERVATIONS6_512: &str = "core_writeposreservations6_512";
pub const CORE_WRITEPOSFULFILLMENTS12_515: &str = "core_writeposfulfillments12_515";
pub const CORE_WRITEPOSSALES21_518: &str = "core_writepossales21_518";
/// Executive-evidence sale row retired from the duplicate `readPosSales3`/`readPosSales4`
/// literals in `pos.ts`. Both literals were byte-identical; the live `readPosSales3` now
/// resolves this owner statement and the dead `readPosSales4` copy was deleted. Read-only
/// caller-transaction `SELECT` with fixed Company/branch/currency/date scope.
pub const CORE_READPOSSALES3_520: &str = "core_readpossales3_520";
/// Plain by-id `pos_sales` row read retired from the three byte-identical
/// `readPosSales1`/`readPosSales2`/`readPosSales5` literals in `pos-daily.ts`,
/// `pos-stripe.ts` and `pos-core.ts`. Read-only caller-transaction `SELECT`
/// scoped by sale id only; no actor/Company threading is required.
pub const CORE_READPOSSALESBYID_521: &str = "core_readpossalesbyid_521";
/// Plain settings-currency read retired from the two byte-identical
/// `SELECT currency FROM pos_settings WHERE company_id=?` literals in
/// `pos-daily.ts`. Read-only caller-transaction `SELECT` scoped by Company.
pub const CORE_READPOSSETTINGSCURRENCY_522: &str = "core_readpossettingscurrency_522";
/// Posting-journal id read retired from the two byte-identical
/// `SELECT journal_id FROM pos_postings WHERE source_key=?` literals in
/// `pos-core.ts` and `pos-stripe.ts`. Read-only caller-transaction `SELECT`
/// scoped by the posting source key; no actor/Company threading is required.
pub const CORE_READPOSPOSTINGSJOURNAL_523: &str = "core_readpospostingsjournal_523";
/// Register-catalog membership read retired from the two byte-identical
/// `SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ...`
/// literals in `pos-core.ts` and `pos-images.ts`. Read-only caller-transaction
/// `SELECT` scoped by product, branch and Company.
pub const CORE_READPOSREGISTERCATALOG_524: &str = "core_readposregistercatalog_524";
/// Plain accounts-json settings read retired from the three byte-identical
/// `SELECT accounts_json FROM pos_settings WHERE company_id=?` literals in
/// `pms-booking-change-facts.ts`, `pms-booking-facts.ts` and
/// `pms-counter-payment-facts.ts`. Read-only caller-transaction `SELECT`
/// scoped by Company.
pub const CORE_READPOSSETTINGSACCOUNTS_525: &str = "core_readpossettingsaccounts_525";
pub const STRIPE_WRITEPOSSTRIPEEVENTS1_49: &str = "stripe_writeposstripeevents1_49";
pub const STRIPE_WRITEPOSSTRIPEEVENTS2_53: &str = "stripe_writeposstripeevents2_53";
pub const STRIPE_WRITEPOSCONNECTIONHISTORY1_58: &str = "stripe_writeposconnectionhistory1_58";
pub const STRIPE_WRITEPOSCONNECTIONS1_72: &str = "stripe_writeposconnections1_72";
pub const STRIPE_WRITEPOSOAUTHSTATES1_82: &str = "stripe_writeposoauthstates1_82";
pub const STRIPE_WRITEPOSPAYMENTS1_86: &str = "stripe_writepospayments1_86";
pub const STRIPE_WRITEPOSPAYMENTS2_96: &str = "stripe_writepospayments2_96";
pub const STRIPE_WRITEPOSPAYMENTS3_100: &str = "stripe_writepospayments3_100";
pub const STRIPE_WRITEPOSPAYMENTS4_110: &str = "stripe_writepospayments4_110";
pub const STRIPE_WRITEPOSREFUNDPAYMENTS1_113: &str = "stripe_writeposrefundpayments1_113";
pub const STRIPE_WRITEPOSCONNECTIONS2_124: &str = "stripe_writeposconnections2_124";
pub const STRIPE_WRITEPOSSTRIPEEVENTS3_134: &str = "stripe_writeposstripeevents3_134";
pub const STRIPE_WRITEPOSSTRIPEEVENTS4_138: &str = "stripe_writeposstripeevents4_138";
pub const STRIPE_WRITEPOSSTRIPEEVENTS5_151: &str = "stripe_writeposstripeevents5_151";
pub const STRIPE_WRITEPOSSETTLEMENTS1_180: &str = "stripe_writepossettlements1_180";
pub const STRIPE_WRITEPOSSETTLEMENTS2_192: &str = "stripe_writepossettlements2_192";
pub const STRIPE_WRITEPOSSETTLEMENTS3_205: &str = "stripe_writepossettlements3_205";
pub const STRIPE_WRITEPOSSETTLEMENTS4_237: &str = "stripe_writepossettlements4_237";
pub const STRIPE_WRITEPOSJOBS1_254: &str = "stripe_writeposjobs1_254";
pub const STRIPE_WRITEPOSPAYMENTS5_268: &str = "stripe_writepospayments5_268";
pub const STRIPE_WRITEPOSPAYMENTS6_277: &str = "stripe_writepospayments6_277";
pub const STRIPE_WRITEPOSREFUNDPAYMENTS2_299: &str = "stripe_writeposrefundpayments2_299";
pub const STRIPE_READPOSJOBS1_307: &str = "stripe_readposjobs1_307";
pub const STRIPE_WRITEPOSJOBS2_312: &str = "stripe_writeposjobs2_312";
pub const STRIPE_WRITEPOSJOBS3_318: &str = "stripe_writeposjobs3_318";
pub const STRIPE_WRITEPOSJOBS4_325: &str = "stripe_writeposjobs4_325";
pub const STRIPE_CONTROLRUN1_364: &str = "stripe_controlrun1_364";
/// Stripe control-route read retired from the three byte-identical
/// `SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?` literals
/// in `pos-stripe.ts`. Read-only caller-transaction `SELECT` scoped by account
/// id and livemode; no actor/Company threading is required.
pub const STRIPE_READPOSSTRIPEROUTES_355: &str = "stripe_readposstriperoutes_355";
pub const DAILY_WRITEPOSSALEDOCUMENTS1_43: &str = "daily_writepossaledocuments1_43";
pub const IMAGES_WRITEPOSPRODUCTIMAGES1_60: &str = "images_writeposproductimages1_60";
/// POS approval-queue (`posWork`) read literals retired from
/// `packages/server-pos/src/adapters/pos.ts`. Read-only: every `pos_read*`
/// statement is a caller-transaction `SELECT` with fixed Company/branch scope
/// and independence filters. The retained `posWork` permission checks stay in
/// the TypeScript application; transport never substitutes for the check.
pub const POS_READPOSREFUNDS3_230: &str = "pos_readposrefunds3_230";
pub const POS_READPOSSHIFTS1_236: &str = "pos_readposshifts1_236";
pub const POS_READPOSCASHMOVEMENTS2_242: &str = "pos_readposcashmovements2_242";
pub const POS_READPOSJOBS1_245: &str = "pos_readposjobs1_245";
pub const ATTACH_STAGEBACKFILLSPECIALIZEDATTACHMENTSEFFECT13_5: &str = "attach_stagebackfillspecializedattachmentseffect13_5";

/// Every kind this owner module accepts. Anything else fails closed.
pub const KINDS: &[&str] = &[
    CORE_SAVEREVERSALREQUEST_41,
    CORE_REVIEWREVERSALREQUEST_42,
    CORE_PREPAREACCOUNTING_43,
    CORE_LINKREGISTER_44,
    CORE_WRITEPOSPOSTINGS1_63,
    CORE_WRITEPOSAUDIT1_73,
    CORE_WRITEPOSJOBS1_81,
    CORE_WRITEPOSRESERVATIONS1_89,
    CORE_WRITEPOSFULFILLMENTS1_92,
    CORE_WRITEPOSRESERVATIONS2_97,
    CORE_WRITEPOSSTOCKMOVES1_112,
    CORE_WRITEPOSFULFILLMENTS2_116,
    CORE_WRITEPOSPAYMENTS1_125,
    CORE_WRITEPOSSALES2_129,
    CORE_WRITEPOSFULFILLMENTS3_144,
    CORE_WRITEPOSSALES3_157,
    CORE_WRITEPOSRECEIPTS1_172,
    CORE_WRITEPOSSETTINGS1_198,
    CORE_WRITEPOSREGISTERS1_206,
    CORE_WRITEPOSOAUTHSTATES1_213,
    CORE_WRITEPOSSETTINGS2_227,
    CORE_WRITEPOSREGISTERS2_242,
    CORE_WRITEPOSREGISTERS3_251,
    CORE_WRITEPOSCATALOG1_271,
    CORE_WRITEPOSCATALOG2_298,
    CORE_WRITEPOSREGISTERCATALOG1_303,
    CORE_WRITEPOSREGISTERCATALOG2_306,
    CORE_WRITEPOSREGISTERCATALOG3_310,
    CORE_WRITEPOSSTOCKMOVES2_352,
    CORE_WRITEPOSROOMCHARGEREVERSALS1_365,
    CORE_WRITEPOSSALES6_369,
    CORE_WRITEPOSSALES7_389,
    CORE_WRITEPOSRESERVATIONS4_392,
    CORE_WRITEPOSFULFILLMENTS10_409,
    CORE_WRITEPOSPAYMENTS5_421,
    CORE_WRITEPOSJOBS2_424,
    CORE_WRITEPOSSETTLEMENTS1_441,
    CORE_WRITEPOSSETTLEMENTS2_452,
    CORE_READPOSREFUNDPAYMENTS2_460,
    CORE_READPOSPAYMENTS14_464,
    CORE_WRITEPOSJOBS3_468,
    CORE_WRITEPOSREFUNDS1_476,
    CORE_READPOSREFUNDPAYMENTS4_479,
    CORE_WRITEPOSJOBS4_483,
    CORE_WRITEPOSREFUNDPAYMENTS2_499,
    CORE_WRITEPOSREFUNDS5_503,
    CORE_WRITEPOSRESERVATIONS6_512,
    CORE_WRITEPOSFULFILLMENTS12_515,
    CORE_WRITEPOSSALES21_518,
    CORE_READPOSSALES3_520,
    CORE_READPOSSALESBYID_521,
    CORE_READPOSSETTINGSCURRENCY_522,
    CORE_READPOSPOSTINGSJOURNAL_523,
    CORE_READPOSREGISTERCATALOG_524,
    CORE_READPOSSETTINGSACCOUNTS_525,
    STRIPE_WRITEPOSSTRIPEEVENTS1_49,
    STRIPE_WRITEPOSSTRIPEEVENTS2_53,
    STRIPE_WRITEPOSCONNECTIONHISTORY1_58,
    STRIPE_WRITEPOSCONNECTIONS1_72,
    STRIPE_WRITEPOSOAUTHSTATES1_82,
    STRIPE_WRITEPOSPAYMENTS1_86,
    STRIPE_WRITEPOSPAYMENTS2_96,
    STRIPE_WRITEPOSPAYMENTS3_100,
    STRIPE_WRITEPOSPAYMENTS4_110,
    STRIPE_WRITEPOSREFUNDPAYMENTS1_113,
    STRIPE_WRITEPOSCONNECTIONS2_124,
    STRIPE_WRITEPOSSTRIPEEVENTS3_134,
    STRIPE_WRITEPOSSTRIPEEVENTS4_138,
    STRIPE_WRITEPOSSTRIPEEVENTS5_151,
    STRIPE_WRITEPOSSETTLEMENTS1_180,
    STRIPE_WRITEPOSSETTLEMENTS2_192,
    STRIPE_WRITEPOSSETTLEMENTS3_205,
    STRIPE_WRITEPOSSETTLEMENTS4_237,
    STRIPE_WRITEPOSJOBS1_254,
    STRIPE_WRITEPOSPAYMENTS5_268,
    STRIPE_WRITEPOSPAYMENTS6_277,
    STRIPE_WRITEPOSREFUNDPAYMENTS2_299,
    STRIPE_READPOSJOBS1_307,
    STRIPE_WRITEPOSJOBS2_312,
    STRIPE_WRITEPOSJOBS3_318,
    STRIPE_WRITEPOSJOBS4_325,
    STRIPE_CONTROLRUN1_364,
    STRIPE_READPOSSTRIPEROUTES_355,
    DAILY_WRITEPOSSALEDOCUMENTS1_43,
    IMAGES_WRITEPOSPRODUCTIMAGES1_60,
    POS_READPOSREFUNDS3_230,
    POS_READPOSSHIFTS1_236,
    POS_READPOSCASHMOVEMENTS2_242,
    POS_READPOSJOBS1_245,
    ATTACH_STAGEBACKFILLSPECIALIZEDATTACHMENTSEFFECT13_5,
    // POS day-control read kinds: closed, SELECT-only owner statements that the
    // TypeScript pms-day-control-facts bridge requests on the caller's unit.
    crate::adapters::day_control_reads::OPEN_SHIFTS,
    crate::adapters::day_control_reads::UNREVIEWED_VARIANCES,
    crate::adapters::day_control_reads::OPEN_BILLS,
    crate::adapters::day_control_reads::PENDING_PAYMENTS,
    crate::adapters::day_control_reads::UNRESOLVED_REFUNDS,
    crate::adapters::day_control_reads::FAILED_JOBS,
    crate::adapters::day_control_reads::MISSING_ROOM_CHARGES,
    crate::adapters::day_control_reads::TOTALS,
    crate::adapters::day_control_reads::STOCK_MOVES,
    crate::adapters::day_control_reads::PENDING_REVERSALS,
    // POS purchase-boundary read kinds: closed, SELECT-only owner statements
    // that the TypeScript purchase bridge requests on the caller's unit.
    crate::adapters::boundary_reads::SALE_CREATE_MODE,
    crate::adapters::boundary_reads::SHIFT_OPEN_MODE,
    crate::adapters::boundary_reads::SALE_ADD_MODE,
    crate::adapters::boundary_reads::SALE_MODE,
    crate::adapters::boundary_reads::CASH_APPROVE_MODE,
    crate::adapters::boundary_reads::SHIFT_MODE,
    crate::adapters::boundary_reads::PAYMENT_MODE,
    crate::adapters::boundary_reads::REFUND_MODE,
    crate::adapters::boundary_reads::RECEIPT_REPLAY,
    // POS core workspace read kinds (shifts family): closed, SELECT-only owner
    // statements that the TypeScript shifts bridge requests on the caller's unit.
    crate::adapters::core_reads::SHIFTS,
    crate::adapters::core_reads::TABLES,
    crate::adapters::core_reads::CASH_MOVEMENTS,
    // POS addon/package obligation read kinds: closed, SELECT-only owner
    // statements that the TypeScript obligation bridge requests on the
    // caller's unit.
    crate::adapters::obligation_reads::ADDON_RESTAURANT,
    crate::adapters::obligation_reads::ADDON_HOTEL,
    crate::adapters::obligation_reads::PACKAGE,
    crate::adapters::obligation_reads::PMS_PACKAGE,
    // POS attachment source read kinds: closed, SELECT-only owner statements
    // that the TypeScript attachment source bridge requests on the caller's unit.
    crate::adapters::attachment_read_sources::POS_SALE,
    crate::adapters::attachment_read_sources::POS_REFUND,
    crate::adapters::attachment_read_sources::POS_SHIFT,
];

pub fn supported(kind: &str) -> bool { KINDS.contains(&kind) }

fn invalid() -> BusinessError { BusinessError::new(FailureKind::InvalidInput, "POS_OWNER_WRITE_INVALID", "Review the POS owner write request.") }

/// Authoritative SQL for one owner write kind, plus the retired read-only
/// approval-queue kinds. Read kinds are `SELECT`-only; the arity check below
/// applies to them exactly like writes.
fn sql(kind: &str) -> Option<&'static str> {
    Some(match kind {
        CORE_SAVEREVERSALREQUEST_41 => r#"INSERT INTO pos_room_charge_reversal_requests(id,sale_id,company_id,branch_id,property_id,sale_version,property_version,amount_minor,reason,restock_line_ids_json,state,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,'requested',?,?,?)"#,
        CORE_REVIEWREVERSALREQUEST_42 => r#"UPDATE pos_room_charge_reversal_requests SET state=?,version=version+1,pos_approved_by=CASE WHEN ?='pos_approved' THEN ? ELSE pos_approved_by END,pms_approved_by=CASE WHEN ?='approved' THEN ? ELSE pms_approved_by END,rejected_by=CASE WHEN ?='rejected' THEN ? ELSE rejected_by END,review_reason=?,updated_at=? WHERE id=?"#,
        CORE_PREPAREACCOUNTING_43 => r#"UPDATE pos_settings SET accounts_json=?,accounting_prepared_by=?,accounting_approved_by=NULL WHERE company_id=?"#,
        CORE_LINKREGISTER_44 => r#"UPDATE pos_registers SET property_id=? WHERE id=?"#,
        CORE_WRITEPOSPOSTINGS1_63 => r#"INSERT INTO pos_postings VALUES(?,?,?)"#,
        CORE_WRITEPOSAUDIT1_73 => r#"INSERT INTO pos_audit VALUES(?,?,?,?,?,?,?,?)"#,
        CORE_WRITEPOSJOBS1_81 => r#"INSERT INTO pos_jobs(id,company_id,kind,record_id,payload_json,created_at) VALUES(?,?,?,?,?,?) ON CONFLICT(kind,record_id) DO NOTHING"#,
        CORE_WRITEPOSRESERVATIONS1_89 => r#"INSERT INTO pos_reservations VALUES(?,?,?,?)"#,
        CORE_WRITEPOSFULFILLMENTS1_92 => r#"UPDATE pos_fulfillments SET reserved=true WHERE id=?"#,
        CORE_WRITEPOSRESERVATIONS2_97 => r#"DELETE FROM pos_reservations WHERE fulfillment_id=? AND product_id=?"#,
        CORE_WRITEPOSSTOCKMOVES1_112 => r#"INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"#,
        CORE_WRITEPOSFULFILLMENTS2_116 => r#"UPDATE pos_fulfillments SET reserved=false,cost_minor=? WHERE id=?"#,
        CORE_WRITEPOSPAYMENTS1_125 => r#"UPDATE pos_payments SET state='succeeded',finance_payment_id=?,updated_at=?,version=version+1 WHERE id=?"#,
        CORE_WRITEPOSSALES2_129 => r#"UPDATE pos_sales SET state='payment_pending',version=version+1,updated_at=? WHERE id=?"#,
        CORE_WRITEPOSFULFILLMENTS3_144 => r#"UPDATE pos_fulfillments SET state='delivered',version=version+1 WHERE id=?"#,
        CORE_WRITEPOSSALES3_157 => r#"UPDATE pos_sales SET state='completed',invoice_id=?,party_id=?,table_id=NULL,version=version+1,updated_at=? WHERE id=?"#,
        CORE_WRITEPOSRECEIPTS1_172 => r#"INSERT INTO pos_receipts VALUES(?,?,?,?,?)"#,
        CORE_WRITEPOSSETTINGS1_198 => r#"INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json,cashier_discount_bps) VALUES(?,true,?,'Asia/Bangkok',?,0)"#,
        CORE_WRITEPOSREGISTERS1_206 => r#"INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES(?,?,?,?,?,?,true)"#,
        CORE_WRITEPOSOAUTHSTATES1_213 => r#"INSERT INTO pos_oauth_states(id,company_id,user_id,expires_at,livemode) VALUES(?,?,?,?,?)"#,
        CORE_WRITEPOSSETTINGS2_227 => r#"INSERT INTO pos_settings(company_id,enabled,currency,timezone,accounts_json,cashier_discount_bps) VALUES(?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET enabled=excluded.enabled,accounts_json=excluded.accounts_json,cashier_discount_bps=excluded.cashier_discount_bps,version=pos_settings.version+1"#,
        CORE_WRITEPOSREGISTERS2_242 => r#"UPDATE pos_registers SET warehouse_id=?,branch_id=?,name=?,mode=?,active=?,version=version+1 WHERE id=?"#,
        CORE_WRITEPOSREGISTERS3_251 => r#"INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES(?,?,?,?,?,?,?)"#,
        CORE_WRITEPOSCATALOG1_271 => r#"UPDATE pos_catalog SET image_id=?,version=version+1 WHERE product_id=?"#,
        CORE_WRITEPOSCATALOG2_298 => r#"INSERT INTO pos_catalog(product_id,company_id,barcode,category,kind,station,recipe_json,modifiers_json,tax_inclusive,enabled,image_id) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(product_id) DO UPDATE SET barcode=excluded.barcode,category=excluded.category,kind=excluded.kind,station=excluded.station,recipe_json=excluded.recipe_json,modifiers_json=excluded.modifiers_json,tax_inclusive=excluded.tax_inclusive,enabled=excluded.enabled,image_id=excluded.image_id,version=pos_catalog.version+1"#,
        CORE_WRITEPOSREGISTERCATALOG1_303 => r#"DELETE FROM pos_register_catalog WHERE product_id=? AND register_id IN (SELECT id FROM pos_registers WHERE company_id=? AND branch_id=?)"#,
        CORE_WRITEPOSREGISTERCATALOG2_306 => r#"DELETE FROM pos_register_catalog WHERE product_id=?"#,
        CORE_WRITEPOSREGISTERCATALOG3_310 => r#"INSERT INTO pos_register_catalog(register_id,product_id) VALUES(?,?)"#,
        CORE_WRITEPOSSTOCKMOVES2_352 => r#"INSERT INTO pos_stock_moves VALUES(?,?,?,?,?,?,?,?,?,?,?,?)"#,
        CORE_WRITEPOSROOMCHARGEREVERSALS1_365 => r#"INSERT INTO pos_room_charge_reversals(sale_id,company_id,property_id,original_entry_id,reversal_entry_id,reason,restocked_lines_json,restored_cost_minor,wasted_cost_minor,reversed_by,reversed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)"#,
        CORE_WRITEPOSSALES6_369 => r#"UPDATE pos_sales SET version=version+1,updated_at=? WHERE id=?"#,
        CORE_WRITEPOSSALES7_389 => r#"UPDATE pos_sales SET state='completed',table_id=NULL,version=version+1,updated_at=? WHERE id=?"#,
        CORE_WRITEPOSRESERVATIONS4_392 => r#"DELETE FROM pos_reservations WHERE fulfillment_id=?"#,
        CORE_WRITEPOSFULFILLMENTS10_409 => r#"UPDATE pos_fulfillments SET state=?,reserved=false,version=version+1 WHERE id=?"#,
        CORE_WRITEPOSPAYMENTS5_421 => r#"UPDATE pos_payments SET payer_email=?,version=version+1,updated_at=? WHERE id=?"#,
        CORE_WRITEPOSJOBS2_424 => r#"UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,last_error=NULL WHERE record_id=? AND kind IN ('checkout','check_payment') AND state='exception'"#,
        CORE_WRITEPOSSETTLEMENTS1_441 => r#"UPDATE pos_settlements SET state=?,journal_id=?,version=version+1 WHERE id=?"#,
        CORE_WRITEPOSSETTLEMENTS2_452 => r#"UPDATE pos_settlements SET bank_transaction_id=?,version=version+1 WHERE id=?"#,
        CORE_READPOSREFUNDPAYMENTS2_460 => r#"UPDATE pos_refund_payments SET provider_id=?,state='pending' WHERE id=? AND provider_id IS NULL RETURNING id"#,
        CORE_READPOSPAYMENTS14_464 => r#"UPDATE pos_payments SET provider_id=?,state='pending' WHERE id=? AND provider_id IS NULL RETURNING id"#,
        CORE_WRITEPOSJOBS3_468 => r#"UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,payload_json=? WHERE id=?"#,
        CORE_WRITEPOSREFUNDS1_476 => r#"UPDATE pos_refunds SET version=version+1 WHERE id=?"#,
        CORE_READPOSREFUNDPAYMENTS4_479 => r#"UPDATE pos_refund_payments SET provider_id=NULL,provider_attempt=provider_attempt+1,state='requested' WHERE id=? AND state='failed' RETURNING id"#,
        CORE_WRITEPOSJOBS4_483 => r#"UPDATE pos_jobs SET state='pending',attempts=0,next_attempt_at=0,last_error=NULL,payload_json=? WHERE id=?"#,
        CORE_WRITEPOSREFUNDPAYMENTS2_499 => r#"UPDATE pos_refund_payments SET state='succeeded' WHERE id=?"#,
        CORE_WRITEPOSREFUNDS5_503 => r#"UPDATE pos_refunds SET state=CASE WHEN EXISTS(SELECT 1 FROM pos_refund_payments WHERE refund_id=? AND state<>'succeeded') THEN 'processing' ELSE 'completed' END,version=version+1 WHERE id=?"#,
        CORE_WRITEPOSRESERVATIONS6_512 => r#"DELETE FROM pos_reservations WHERE fulfillment_id IN (SELECT f.id FROM pos_fulfillments f JOIN pos_lines l ON l.fulfillment_id=f.id WHERE l.sale_id=? AND f.state='draft')"#,
        CORE_WRITEPOSFULFILLMENTS12_515 => r#"UPDATE pos_fulfillments SET reserved=false WHERE state='draft' AND id IN (SELECT fulfillment_id FROM pos_lines WHERE sale_id=?)"#,
        CORE_WRITEPOSSALES21_518 => r#"UPDATE pos_sales SET state='open',version=version+1 WHERE id=? AND state='payment_pending'"#,
        CORE_READPOSSALES3_520 => r#"SELECT s.id source_id,s.id sale_id,'sale' source_type,s.number receipt_number,s.business_date,
     NULL::text accounting_date,s.currency,s.state,s.total_minor::text amount_minor,s.total_minor::text contribution_minor,
     '0'::text refunded_minor,s.version revision,s.version sale_version,s.branch_id
   FROM pos_sales s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' LIMIT 1"#,
        CORE_READPOSSALESBYID_521 => r#"SELECT * FROM pos_sales WHERE id=?"#,
        CORE_READPOSSETTINGSCURRENCY_522 => r#"SELECT currency FROM pos_settings WHERE company_id=?"#,
        CORE_READPOSPOSTINGSJOURNAL_523 => r#"SELECT journal_id FROM pos_postings WHERE source_key=?"#,
        CORE_READPOSREGISTERCATALOG_524 => r#"SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.branch_id=? AND r.company_id=?"#,
        CORE_READPOSSETTINGSACCOUNTS_525 => r#"SELECT accounts_json FROM pos_settings WHERE company_id=?"#,
        STRIPE_WRITEPOSSTRIPEEVENTS1_49 => r#"INSERT INTO pos_stripe_events(id,account_id,livemode,company_id,event_type,payload_json,received_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING"#,
        STRIPE_WRITEPOSSTRIPEEVENTS2_53 => r#"UPDATE pos_stripe_events SET provider_created_at=COALESCE(provider_created_at,?) WHERE id=?"#,
        STRIPE_WRITEPOSCONNECTIONHISTORY1_58 => r#"INSERT INTO pos_connection_history VALUES(?,?,?) ON CONFLICT DO NOTHING"#,
        STRIPE_WRITEPOSCONNECTIONS1_72 => r#"INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,requirements_json,state,payout_interval,thb_bank_ready,last_checked_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET account_id=excluded.account_id,livemode=excluded.livemode,charges_enabled=excluded.charges_enabled,payouts_enabled=excluded.payouts_enabled,card_enabled=excluded.card_enabled,promptpay_enabled=excluded.promptpay_enabled,requirements_json=excluded.requirements_json,state=excluded.state,payout_interval=excluded.payout_interval,thb_bank_ready=excluded.thb_bank_ready,last_checked_at=excluded.last_checked_at,version=pos_connections.version+1"#,
        STRIPE_WRITEPOSOAUTHSTATES1_82 => r#"UPDATE pos_oauth_states SET used_at=? WHERE id=?"#,
        STRIPE_WRITEPOSPAYMENTS1_86 => r#"UPDATE pos_payments SET provider_received_at=COALESCE(provider_received_at,?) WHERE id=?"#,
        STRIPE_WRITEPOSPAYMENTS2_96 => r#"UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?"#,
        STRIPE_WRITEPOSPAYMENTS3_100 => r#"UPDATE pos_payments SET provider_received_at=COALESCE(provider_received_at,?) WHERE id=?"#,
        STRIPE_WRITEPOSPAYMENTS4_110 => r#"UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?"#,
        STRIPE_WRITEPOSREFUNDPAYMENTS1_113 => r#"UPDATE pos_refund_payments SET state='failed' WHERE id=?"#,
        STRIPE_WRITEPOSCONNECTIONS2_124 => r#"UPDATE pos_connections SET state='disconnected',charges_enabled=false,payouts_enabled=false,version=version+1 WHERE company_id=? AND account_id=? AND livemode=?"#,
        STRIPE_WRITEPOSSTRIPEEVENTS3_134 => r#"UPDATE pos_stripe_events SET applied_at=? WHERE id=?"#,
        STRIPE_WRITEPOSSTRIPEEVENTS4_138 => r#"UPDATE pos_stripe_events SET applied_at=? WHERE id=?"#,
        STRIPE_WRITEPOSSTRIPEEVENTS5_151 => r#"UPDATE pos_stripe_events SET applied_at=? WHERE id=?"#,
        STRIPE_WRITEPOSSETTLEMENTS1_180 => r#"INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,created_at) VALUES(?,?,?,?,'balance',?,?,?,'THB','unmapped',?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO NOTHING"#,
        STRIPE_WRITEPOSSETTLEMENTS2_192 => r#"INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,journal_id,created_at) VALUES(?,?,?,?,'balance',?,?,?,'THB','recorded',?,?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO UPDATE SET state='recorded',journal_id=excluded.journal_id,details_json=excluded.details_json WHERE pos_settlements.state='unmapped'"#,
        STRIPE_WRITEPOSSETTLEMENTS3_205 => r#"INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,created_at) VALUES(?,?,?,?,'payout',?, ?,0,'THB','unmapped',?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO UPDATE SET state='unmapped',details_json=excluded.details_json WHERE pos_settlements.journal_id IS NULL"#,
        STRIPE_WRITEPOSSETTLEMENTS4_237 => r#"INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,currency,state,details_json,journal_id,created_at) VALUES(?,?,?,?,?,?,?,\'THB\',?,?,?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO UPDATE SET state=excluded.state,details_json=excluded.details_json,journal_id=COALESCE(excluded.journal_id,pos_settlements.journal_id)"#,
        STRIPE_WRITEPOSJOBS1_254 => r#"UPDATE pos_jobs SET payload_json=? WHERE id=?"#,
        STRIPE_WRITEPOSPAYMENTS5_268 => r#"UPDATE pos_payments SET provider_id=?,qr_data=?,expires_at=?,state='pending',version=version+1,updated_at=? WHERE id=? AND state IN ('requested','exception')"#,
        STRIPE_WRITEPOSPAYMENTS6_277 => r#"UPDATE pos_payments SET provider_id=?,checkout_url=?,state='pending',version=version+1,updated_at=? WHERE id=? AND state IN ('requested','exception')"#,
        STRIPE_WRITEPOSREFUNDPAYMENTS2_299 => r#"UPDATE pos_refund_payments SET provider_id=?,state='pending' WHERE id=?"#,
        STRIPE_READPOSJOBS1_307 => r#"UPDATE pos_jobs SET state='running',lease_token=?,lease_until=?,attempts=attempts+1 WHERE id=(SELECT id FROM pos_jobs WHERE kind<>'complete_sale' AND (state='pending' OR state='running' AND lease_until<?) AND next_attempt_at<=? AND (?::text IS NULL OR company_id=?) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *"#,
        STRIPE_WRITEPOSJOBS2_312 => r#"UPDATE pos_jobs SET lease_until=? WHERE id=? AND lease_token=? AND state='running'"#,
        STRIPE_WRITEPOSJOBS3_318 => r#"UPDATE pos_jobs SET state=?,next_attempt_at=?,attempts=0,lease_until=NULL,lease_token=NULL,last_error=NULL WHERE id=? AND lease_token=?"#,
        STRIPE_WRITEPOSJOBS4_325 => r#"UPDATE pos_jobs SET state=?,next_attempt_at=?,last_error=?,lease_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?"#,
        STRIPE_CONTROLRUN1_364 => r#"INSERT INTO pos_stripe_routes VALUES(?,?,?,?,?) ON CONFLICT(account_id,livemode) DO NOTHING"#,
        STRIPE_READPOSSTRIPEROUTES_355 => r#"SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?"#,
        DAILY_WRITEPOSSALEDOCUMENTS1_43 => r#"INSERT INTO pos_sale_documents(sale_id,company_id,branch_id,snapshot_json,created_at) VALUES(?,?,?,?::jsonb,?) ON CONFLICT(sale_id) DO NOTHING"#,
        IMAGES_WRITEPOSPRODUCTIMAGES1_60 => r#"INSERT INTO pos_product_images(id,company_id,product_id,object_key,thumbnail_key,sha256,width,height,mime_type,source,metadata_json,created_by,created_at,
   attachment_source_id,attachment_link_id,attachment_set_id,thumbnail_checksum_sha256) VALUES(?,?,?,?,?,?,?,?,?,?,?::jsonb,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING"#,
        POS_READPOSREFUNDS3_230 => r#"SELECT * FROM pos_refunds WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='requested' AND requested_by<>?"#,
        POS_READPOSSHIFTS1_236 => r#"SELECT * FROM pos_shifts WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='closed' AND opened_by<>?"#,
        POS_READPOSCASHMOVEMENTS2_242 => r#"SELECT m.* FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND m.approved_by IS NULL AND m.created_by<>?"#,
        POS_READPOSJOBS1_245 => r#"SELECT * FROM pos_jobs WHERE company_id=? AND state='exception'"#,
        ATTACH_STAGEBACKFILLSPECIALIZEDATTACHMENTSEFFECT13_5 => r#"UPDATE pos_product_images SET attachment_set_id=?,attachment_link_id=?,attachment_source_id=?,thumbnail_checksum_sha256=? WHERE id=? AND attachment_source_id IS NULL"#,
        // Day-control reads: authoritative owner SQL lives in the dedicated
        // module and is shared with the integration parity oracle.
        kind if crate::adapters::day_control_reads::supported(kind) =>
            return crate::adapters::day_control_reads::sql(kind),
        // Purchase-boundary reads: authoritative owner SQL lives in the
        // dedicated module and is shared with the integration parity oracle.
        kind if crate::adapters::boundary_reads::supported(kind) =>
            return crate::adapters::boundary_reads::sql(kind),
        // POS core workspace reads (shifts family): authoritative owner SQL
        // lives in the dedicated module shared with the integration oracle.
        kind if crate::adapters::core_reads::supported(kind) =>
            return crate::adapters::core_reads::sql(kind),
        // Addon/package obligation reads: authoritative owner SQL lives in the
        // dedicated module shared with the integration parity oracle.
        kind if crate::adapters::obligation_reads::supported(kind) =>
            return crate::adapters::obligation_reads::sql(kind),
        // Attachment source reads: authoritative owner SQL lives in the
        // dedicated module shared with the integration parity oracle.
        kind if crate::adapters::attachment_read_sources::supported(kind) =>
            return crate::adapters::attachment_read_sources::sql(kind),
        _ => return None,
    })
}

async fn ready(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction()
        || sqlx::query_scalar::<_, String>("SHOW transaction_isolation")
            .fetch_one(&mut *connection)
            .await?
            != "serializable"
    {
        return Err(erpx_server_runtime::public::postgres::PersistenceError::new(
            "POS_OWNER_WRITE_SERIALIZABLE_REQUIRED",
        )
        .into());
    }
    Ok(())
}

/// Build the authoritative write statement for `kind` on the caller's unit.
/// `kind` is allow-listed here and the SQL is selected here; only positional
/// values come from the caller. Placeholder count is derived from owner SQL.
pub async fn owner_write_statement(
    connection: &mut PgConnection,
    _company: &CompanyId,
    kind: &str,
    values: Vec<Value>,
) -> Result<PosOwnerStatement, Error> {
    ready(connection).await?;
    if !supported(kind) {
        return Err(invalid().into());
    }
    let statement = sql(kind).ok_or_else(invalid)?;
    if values.len() != statement.matches('?').count() {
        return Err(invalid().into());
    }
    Ok(PosOwnerStatement { sql: statement.to_owned(), values })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_kind_is_allow_listed_with_parameterized_owner_sql() {
        assert_eq!(KINDS.len(), 119);
        for kind in KINDS {
            assert!(supported(kind), "kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.contains('?'), "write must be parameterized: {kind}");
        }
    }
    #[test]
    fn unknown_kind_fails_closed() {
        assert!(!supported("not_a_pos_owner_write"));
        assert!(sql("not_a_pos_owner_write").is_none());
    }
    /// The retired approval-queue read port is read-only: fixed `SELECT`
    /// statements with Company/branch scope and independence filters, never a
    /// mutation. The retained `posWork` permission checks stay in the
    /// TypeScript application; transport only selects the statement text.
    #[test]
    fn approval_queue_read_kinds_are_select_only() {
        for kind in [
            POS_READPOSREFUNDS3_230,
            POS_READPOSSHIFTS1_236,
            POS_READPOSCASHMOVEMENTS2_242,
            POS_READPOSJOBS1_245,
        ] {
            assert!(supported(kind), "read kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(
                stmt.starts_with("SELECT "),
                "read kind must be SELECT-only: {kind}"
            );
            for forbidden in ["INSERT", "UPDATE", "DELETE", "RETURNING", ";"] {
                assert!(
                    !stmt.contains(forbidden),
                    "read kind must not mutate: {kind} contains {forbidden}"
                );
            }
        }
        assert_eq!(
            sql(POS_READPOSREFUNDS3_230).unwrap(),
            "SELECT * FROM pos_refunds WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='requested' AND requested_by<>?"
        );
        assert_eq!(
            sql(POS_READPOSSHIFTS1_236).unwrap(),
            "SELECT * FROM pos_shifts WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='closed' AND opened_by<>?"
        );
        assert_eq!(
            sql(POS_READPOSCASHMOVEMENTS2_242).unwrap(),
            "SELECT m.* FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND m.approved_by IS NULL AND m.created_by<>?"
        );
        assert_eq!(
            sql(POS_READPOSJOBS1_245).unwrap(),
            "SELECT * FROM pos_jobs WHERE company_id=? AND state='exception'"
        );
    }

    /// The retired executive-evidence sale row is a SELECT-only owner statement.
    /// The SQL is byte-identical to the deleted byte-for-byte duplicate
    /// `readPosSales3`/`readPosSales4` literals in `pos.ts`; the live kind is the
    /// sole authority and the dead copy was removed with the TypeScript SQL.
    #[test]
    fn executive_evidence_sale_read_kind_is_select_only() {
        assert!(supported(CORE_READPOSSALES3_520));
        let stmt = sql(CORE_READPOSSALES3_520).unwrap();
        assert!(stmt.starts_with("SELECT "), "evidence read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "evidence read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 7, "evidence read carries id/company/branch/branch2/currency/from/to scope");
        assert_eq!(
            stmt,
            "SELECT s.id source_id,s.id sale_id,'sale' source_type,s.number receipt_number,s.business_date,\n     NULL::text accounting_date,s.currency,s.state,s.total_minor::text amount_minor,s.total_minor::text contribution_minor,\n     '0'::text refunded_minor,s.version revision,s.version sale_version,s.branch_id\n   FROM pos_sales s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?\n     AND s.business_date BETWEEN ? AND ? AND s.state='completed' LIMIT 1"
        );
    }

    /// The retired plain by-id sale row is a SELECT-only owner statement. Its
    /// SQL is byte-identical to the three deleted `SELECT * FROM pos_sales
    /// WHERE id=?` literals in `pos-core.ts`/`pos-daily.ts`/`pos-stripe.ts`;
    /// this kind is the sole authority and every TypeScript copy was removed.
    #[test]
    fn sale_by_id_read_kind_is_select_only() {
        assert!(supported(CORE_READPOSSALESBYID_521));
        let stmt = sql(CORE_READPOSSALESBYID_521).unwrap();
        assert!(stmt.starts_with("SELECT "), "by-id sale read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "by-id sale read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "by-id sale read carries only the sale id");
        assert_eq!(stmt, "SELECT * FROM pos_sales WHERE id=?");
    }

    /// The retired Stripe control-route read is a SELECT-only owner statement.
    /// Its SQL is byte-identical to the three deleted
    /// `SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?`
    /// literals in `pos-stripe.ts`; this kind is the sole authority and every
    /// TypeScript copy was removed.
    #[test]
    fn stripe_route_read_kind_is_select_only() {
        assert!(supported(STRIPE_READPOSSTRIPEROUTES_355));
        let stmt = sql(STRIPE_READPOSSTRIPEROUTES_355).unwrap();
        assert!(stmt.starts_with("SELECT "), "stripe-route read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "stripe-route read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 2, "stripe-route read carries account id and livemode");
        assert_eq!(stmt, "SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?");
    }

    /// The retired settings-currency read is a SELECT-only owner statement. Its
    /// SQL is byte-identical to the two deleted `SELECT currency FROM
    /// pos_settings WHERE company_id=?` literals in `pos-daily.ts`; this kind is
    /// the sole authority and every TypeScript copy was removed.
    #[test]
    fn settings_currency_read_kind_is_select_only() {
        assert!(supported(CORE_READPOSSETTINGSCURRENCY_522));
        let stmt = sql(CORE_READPOSSETTINGSCURRENCY_522).unwrap();
        assert!(stmt.starts_with("SELECT "), "currency read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "currency read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "currency read carries only the Company");
        assert_eq!(stmt, "SELECT currency FROM pos_settings WHERE company_id=?");
    }

    /// The retired posting-journal id read is a SELECT-only owner statement. Its
    /// SQL is byte-identical to the two deleted `SELECT journal_id FROM
    /// pos_postings WHERE source_key=?` literals in `pos-core.ts`/`pos-stripe.ts`;
    /// this kind is the sole authority and every TypeScript copy was removed.
    #[test]
    fn posting_journal_read_kind_is_select_only() {
        assert!(supported(CORE_READPOSPOSTINGSJOURNAL_523));
        let stmt = sql(CORE_READPOSPOSTINGSJOURNAL_523).unwrap();
        assert!(stmt.starts_with("SELECT "), "posting-journal read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "posting-journal read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "posting-journal read carries only the source key");
        assert_eq!(stmt, "SELECT journal_id FROM pos_postings WHERE source_key=?");
    }

    /// The retired register-catalog membership read is a SELECT-only owner
    /// statement. Its SQL is byte-identical to the two deleted
    /// `SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ...`
    /// literals in `pos-core.ts`/`pos-images.ts`; this kind is the sole
    /// authority and every TypeScript copy was removed.
    #[test]
    fn register_catalog_read_kind_is_select_only() {
        assert!(supported(CORE_READPOSREGISTERCATALOG_524));
        let stmt = sql(CORE_READPOSREGISTERCATALOG_524).unwrap();
        assert!(stmt.starts_with("SELECT "), "register-catalog read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "register-catalog read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 3, "register-catalog read carries product, branch and Company scope");
        assert_eq!(
            stmt,
            "SELECT rc.product_id FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=? AND r.branch_id=? AND r.company_id=?"
        );
    }

    /// The retired accounts-json settings read is a SELECT-only owner
    /// statement. Its SQL is byte-identical to the three deleted
    /// `SELECT accounts_json FROM pos_settings WHERE company_id=?` literals in
    /// `pms-booking-change-facts.ts`/`pms-booking-facts.ts`/
    /// `pms-counter-payment-facts.ts`; this kind is the sole authority and
    /// every TypeScript copy was removed.
    #[test]
    fn settings_accounts_read_kind_is_select_only() {
        assert!(supported(CORE_READPOSSETTINGSACCOUNTS_525));
        let stmt = sql(CORE_READPOSSETTINGSACCOUNTS_525).unwrap();
        assert!(stmt.starts_with("SELECT "), "accounts read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "accounts read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "accounts read carries only the Company");
        assert_eq!(stmt, "SELECT accounts_json FROM pos_settings WHERE company_id=?");
    }

    /// The retired purchase-boundary reads are SELECT-only owner statements.
    /// No caller-supplied SQL is accepted; transport selects the statement text.
    #[test]
    fn boundary_read_kinds_are_select_only() {
        for kind in crate::adapters::boundary_reads::KINDS {
            assert!(supported(kind), "boundary read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "boundary kind must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE", "DELETE", "RETURNING", ";"] {
                assert!(
                    !stmt.contains(forbidden),
                    "boundary kind must not mutate: {kind} contains {forbidden}"
                );
            }
        }
        assert_eq!(crate::adapters::boundary_reads::KINDS.len(), 9);
    }

    /// The retired core workspace reads (shifts family) are SELECT-only owner
    /// statements scoped by caller-supplied Company/branch values. No caller
    /// SQL is accepted; transport only selects the statement text.
    #[test]
    fn core_read_kinds_are_select_only() {
        for kind in crate::adapters::core_reads::KINDS {
            assert!(supported(kind), "core read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "core kind must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(
                    !stmt.contains(forbidden),
                    "core kind must not mutate: {kind} contains {forbidden}"
                );
            }
        }
        assert_eq!(crate::adapters::core_reads::KINDS.len(), 3);
    }

    /// The retired addon/package obligation reads are SELECT-only owner
    /// statements scoped by caller-supplied Company/package values. No caller
    /// SQL is accepted; transport only selects the statement text.
    #[test]
    fn obligation_read_kinds_are_select_only() {
        for kind in crate::adapters::obligation_reads::KINDS {
            assert!(supported(kind), "obligation read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("WITH scope AS (SELECT "), "obligation kind must be a scoped read: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
                assert!(
                    !stmt.contains(forbidden),
                    "obligation kind must not mutate: {kind} contains {forbidden}"
                );
            }
        }
        assert_eq!(crate::adapters::obligation_reads::KINDS.len(), 4);
    }
}
