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
/// Plain by-company connection row read retired from the three byte-identical
/// `SELECT * FROM pos_connections WHERE company_id=?` literals in
/// `pos-stripe.ts` (`readPosConnections1`/`3`/`4`). Read-only caller-transaction
/// `SELECT` scoped by Company; no actor threading is required.
pub const STRIPE_READPOSCONNECTIONS_526: &str = "stripe_readposconnections_526";
/// Caller-unit by-id payment row with the original Stripe row lock. Byte-identical
/// to the four `readPosPayments1`-`4` `SELECT * FROM pos_payments WHERE id=?
/// FOR UPDATE` literals deleted from `pos-stripe.ts`. The `FOR UPDATE` clause is
/// executed on the caller's serializable unit, so the lock is held by the caller
/// exactly as before; no TypeScript copy or fallback reader exists for this kind.
pub const STRIPE_READPOSPAYMENTSBYID_527: &str = "stripe_readpospaymentsbyid_527";
/// PMS booking connection-readiness read retired from the two byte-identical
/// `SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND
/// charges_enabled=true AND payouts_enabled=true` literals in
/// `pms-booking-change-facts.ts` and `pms-booking-facts.ts`. Read-only
/// caller-transaction `SELECT` scoped by Company; no TypeScript mirror remains.
pub const STRIPE_READPOSBOOKINGCONNECTION_528: &str = "stripe_readposbookingconnection_528";
/// Stripe entrypoint connection-readiness jobs read retired from the byte-identical
/// `SELECT id,kind,state,last_error,payload_json FROM pos_jobs WHERE company_id=?
/// AND kind IN ('connect','connect_refresh') ORDER BY created_at DESC LIMIT 10`
/// literal in `stripe-entrypoint-facts.ts`. Read-only caller-transaction `SELECT`
/// scoped by Company; no TypeScript mirror remains. The companion connection
/// read reuses the byte-identical `stripe_readposconnections_526` kind.
pub const STRIPE_READPOSJOBSENTRYPOINT_529: &str = "stripe_readposjobsentrypoint_529";
/// POS presentation movement-sale projection retired from the literal in
/// `movement-presentation-query.ts`. Resolves the original sale for a stock
/// movement within the caller's Company and optional Branch fence, preserving
/// the first-sale ordering. Read-only caller-transaction read scoped by
/// movement id, Company and optional Branch; no TypeScript mirror remains.
pub const POS_READPOSMOVEMENTSALE_530: &str = "pos_readposmovementsale_530";
/// POS product-image catalog guard read retired from the literal in
/// `pos-images.ts`. Byte-identical `SELECT product_id,version FROM pos_catalog
/// WHERE product_id=? AND company_id=?` used to validate the current catalog
/// version before saving a product image. Read-only caller-transaction read
/// scoped by product and Company; no TypeScript mirror remains.
pub const IMAGES_READPOSCATALOG_531: &str = "images_readposcatalog_531";
/// POS product-image attachment-source guard read retired from the literal in
/// `pos-images.ts`. Byte-identical `SELECT attachment_source_id FROM
/// pos_product_images WHERE id=? AND company_id=? AND product_id=?` used to
/// detect an existing product image before a versioned save. Read-only
/// caller-transaction read scoped by image, Company and product; no TypeScript
/// mirror remains.
pub const IMAGES_READPOSPRODUCTIMAGES_532: &str = "images_readposproductimages_532";
/// PMS PromptPay/daily-payout connection probe retired from the byte-identical
/// `readPosConnections` literal in `pms-command-facts.ts`. Read-only
/// caller-transaction `SELECT` scoped by Company; no TypeScript mirror remains.
pub const PMS_READPOSCONNECTIONSPROMPTPAY_533: &str = "pms_readposconnectionspromptpay_533";
/// PMS card/PromptPay charge-readiness probe retired from the byte-identical
/// `readPosConnections2` literal in `pms-command-facts.ts`. Read-only
/// caller-transaction `SELECT` scoped by Company and the livemode fact; no
/// TypeScript mirror remains.
pub const PMS_READPOSCONNECTIONSCHARGE_534: &str = "pms_readposconnectionscharge_534";
/// PMS connection account/livemode probe retired from the byte-identical
/// `readPosConnections3` literal in `pms-command-facts.ts`. Read-only
/// caller-transaction `SELECT` scoped by Company; no TypeScript mirror remains.
pub const PMS_READPOSCONNECTIONSACCOUNT_535: &str = "pms_readposconnectionsaccount_535";
/// Counter-payment connection readiness read retired from the byte-identical
/// `readPosConnections` literal in `pms-counter-payment-facts.ts`. Read-only
/// caller-transaction `SELECT` scoped by Company; no TypeScript mirror remains.
pub const COUNTER_READPOSCONNECTIONREADINESS_536: &str = "counter_readposconnectionreadiness_536";
/// Hospitality-setup settings read retired from the `SELECT enabled,currency
/// FROM pos_settings WHERE company_id=?` literal in
/// `pms-hospitality-setup-facts.ts`. Read-only caller-transaction `SELECT`
/// scoped by Company; no TypeScript mirror remains.
pub const CORE_READPOSSETTINGSENABLEDCURRENCY_537: &str = "core_readpossettingsenabledcurrency_537";
/// Hospitality-setup restaurant register list retired from the
/// `SELECT id,name,warehouse_id,mode FROM pos_registers WHERE company_id=?
/// AND branch_id=? AND active=true AND mode='restaurant'` literal in
/// `pms-hospitality-setup-facts.ts`. Read-only caller-transaction `SELECT`
/// scoped by Company and Branch; no TypeScript mirror remains.
pub const CORE_READPOSRESTAURANTREGISTERS_538: &str = "core_readposrestaurantregisters_538";
/// By-id register row read retired from the two byte-identical
/// `SELECT * FROM pos_registers WHERE id=?` literals in `pos.ts`
/// (`readPosRegisters3`) and `pos-core.ts` (`readPosRegisters6`). Read-only
/// caller-transaction `SELECT` scoped by register id; no TypeScript mirror
/// remains. The lock-bearing by-id register reads and refund reads are
/// unchanged and stay in TypeScript.
pub const CORE_READPOSREGISTERBYID_539: &str = "core_readposregisterbyid_539";
/// Plain by-id-and-company payment row read retired from the two byte-identical
/// `SELECT * FROM pos_payments WHERE id=? AND company_id=?` literals in
/// `pos-stripe.ts` (`readPosPayments8`/`readPosPayments9`, checkout flows).
/// Read-only caller-transaction `SELECT`. The `FOR UPDATE` by-id payment reads
/// and refund reads are unchanged and stay in TypeScript.
pub const STRIPE_READPOSPAYMENTBYIDCOMPANY_540: &str = "stripe_readpospaymentbyidcompany_540";
/// Plain by-id payment row read retired from the three byte-identical
/// `SELECT * FROM pos_payments WHERE id=?` literals in `pos-stripe.ts`
/// (`readPosPayments6`/`readPosPayments11`) and `pos-core.ts`
/// (`readPosPayments16`). Read-only caller-transaction `SELECT` scoped by
/// payment id only. The `FOR UPDATE` by-id reads and refund reads are
/// unchanged and stay in TypeScript.
pub const CORE_READPOSPAYMENTBYID_541: &str = "core_readpospaymentbyid_541";
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
    STRIPE_READPOSCONNECTIONS_526,
    STRIPE_READPOSPAYMENTSBYID_527,
    STRIPE_READPOSBOOKINGCONNECTION_528,
    STRIPE_READPOSJOBSENTRYPOINT_529,
    POS_READPOSMOVEMENTSALE_530,
    IMAGES_READPOSCATALOG_531,
    IMAGES_READPOSPRODUCTIMAGES_532,
    PMS_READPOSCONNECTIONSPROMPTPAY_533,
    PMS_READPOSCONNECTIONSCHARGE_534,
    PMS_READPOSCONNECTIONSACCOUNT_535,
    COUNTER_READPOSCONNECTIONREADINESS_536,
    CORE_READPOSSETTINGSENABLEDCURRENCY_537,
    CORE_READPOSRESTAURANTREGISTERS_538,
    CORE_READPOSREGISTERBYID_539,
    STRIPE_READPOSPAYMENTBYIDCOMPANY_540,
    CORE_READPOSPAYMENTBYID_541,
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
    // POS day-report business-date list kinds: closed, SELECT-only owner
    // statements that the TypeScript day-report bridge requests on the
    // caller's unit. Lock-bearing day-close reads stay inline in TypeScript.
    crate::adapters::day_report_reads::DAY_CLOSES,
    crate::adapters::day_report_reads::SALES,
    crate::adapters::day_report_reads::SHIFTS,
    crate::adapters::day_report_reads::PAYMENTS,
    crate::adapters::day_report_reads::REFUNDS,
    crate::adapters::day_report_reads::REFUND_PAYMENTS,
    crate::adapters::day_report_reads::STOCK_MOVES,
    crate::adapters::day_report_reads::FULFILLMENTS,
    crate::adapters::day_report_reads::CASH_MOVEMENTS,
    crate::adapters::day_report_reads::SETTLEMENTS,
    // POS sale-child detail read kinds: closed, SELECT-only owner statements
    // keyed by sale id. Lock-bearing, PMS-fragment and refund-child reads stay
    // inline in the TypeScript callers.
    crate::adapters::sale_child_reads::LINES_WITH_FULFILLMENT,
    crate::adapters::sale_child_reads::PAYMENTS_ORDERED,
    crate::adapters::sale_child_reads::LINES_ALL,
    crate::adapters::sale_child_reads::PAYMENTS_SUCCEEDED_TOTAL,
    crate::adapters::sale_child_reads::PAYMENT_PENDING_ID,
    crate::adapters::sale_child_reads::PAYMENTS_REFUNDABLE,
    crate::adapters::sale_child_reads::PAYMENT_ID,
    crate::adapters::sale_child_reads::LINES_FULFILLMENT_DETAIL,
    crate::adapters::sale_child_reads::PAYMENT_FIRST_ID,
    crate::adapters::sale_child_reads::LINES_TOTAL,
    crate::adapters::sale_child_reads::PAYMENT_OPEN_ID,
    // POS child-tail read kinds (round 30): closed, SELECT-only, lock-free
    // owner statements ported from the inline pos-core.ts literals. Row locks,
    // PMS fragments and the day-report/Stripe registries stay inline.
    crate::adapters::child_tail_reads::LINES_SUM,
    crate::adapters::child_tail_reads::CREATED_BY,
    crate::adapters::child_tail_reads::COMPLETED_LINE,
    crate::adapters::child_tail_reads::REFUND_PAYMENT,
    // POS list-read kinds (round 31): closed, SELECT-only, lock-free owner
    // statements ported from the inline pos.ts literals. Row locks, PMS
    // fragments and the day-report/Stripe registries stay inline.
    crate::adapters::list_reads::SALES,
    crate::adapters::list_reads::REFUNDS,
    crate::adapters::list_reads::CONNECTIONS,
    crate::adapters::list_reads::SETTLEMENTS,
    // POS hotel day-assertion property kinds (round 32): closed, SELECT-only,
    // Company-scoped, lock-free owner statements ported from the inline
    // pos-core.ts literals. Row locks, PMS fragments and the day-report/Stripe
    // registries stay inline.
    crate::adapters::hotel_reads::REGISTER_PROPERTY,
    crate::adapters::hotel_reads::FULFILLMENT_PROPERTY,
    // POS Stripe settlement/provider read kinds: closed, SELECT-only owner
    // statements scoped by caller-supplied company/account/livemode + ids.
    // Row locks, advisory/LOCK, PMS reads and the day-report registry stay
    // inline in their TypeScript callers and dedicated modules.
    crate::adapters::stripe_reads::SETTLEMENT_BY_KEY,
    crate::adapters::stripe_reads::SETTLEMENT_BALANCE_STATE,
    crate::adapters::stripe_reads::SETTLEMENT_PAYOUT_JOURNAL,
    crate::adapters::stripe_reads::SETTLEMENT_BALANCE_ROW,
    crate::adapters::stripe_reads::PAYMENT_BY_ID,
    crate::adapters::stripe_reads::PAYMENT_ID,
    crate::adapters::stripe_reads::REFUND_BY_ID,
    crate::adapters::stripe_reads::REFUND_ID_BY_PROVIDER,
    // POS audit actor read kinds: closed, SELECT-only owner statements on the
    // owner's pos_audit table. Row locks, advisory/LOCK, PMS reads and the
    // day-report/Stripe registries stay inline in their callers and modules.
    crate::adapters::audit_reads::AUDIT_ACTOR_CHARGE_TO_ROOM,
    crate::adapters::audit_reads::AUDIT_ACTOR_KITCHEN_START,
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
    // POS keyed register/shift/table reads: closed, SELECT-only, lock-free
    // owner statements ported from the inline TypeScript literals.
    crate::adapters::core_reads::REGISTER_BY_NAME,
    crate::adapters::core_reads::OPEN_SHIFT_BY_REGISTER,
    crate::adapters::core_reads::TABLE_BY_NAME,
    crate::adapters::core_reads::REGISTERS_BY_KIND,
    crate::adapters::core_reads::RESTAURANT_REGISTER_PROPERTY,
    crate::adapters::core_reads::OPEN_SHIFT_BY_ID,
    // POS keyed core settings/receipt/day-close/reversal reads: closed,
    // SELECT-only, lock-free owner statements ported from inline TS literals.
    crate::adapters::core_keyed_reads::SETTINGS,
    crate::adapters::core_keyed_reads::SETTINGS_CURRENCY_ACCOUNTS,
    crate::adapters::core_keyed_reads::SETTINGS_PREPARED_STATE,
    crate::adapters::core_keyed_reads::RECEIPT_BY_ID,
    crate::adapters::core_keyed_reads::SALE_DOCUMENT,
    crate::adapters::core_keyed_reads::PREPARED_DAY_CLOSE,
    crate::adapters::core_keyed_reads::PENDING_REVERSAL_REQUEST,
    crate::adapters::core_keyed_reads::REVERSAL_BY_SALE,
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
        STRIPE_READPOSCONNECTIONS_526 => r#"SELECT * FROM pos_connections WHERE company_id=?"#,
        STRIPE_READPOSPAYMENTSBYID_527 => r#"SELECT * FROM pos_payments WHERE id=? FOR UPDATE"#,
        STRIPE_READPOSBOOKINGCONNECTION_528 => r#"SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true"#,
        STRIPE_READPOSJOBSENTRYPOINT_529 => r#"SELECT id,kind,state,last_error,payload_json FROM pos_jobs WHERE company_id=? AND kind IN ('connect','connect_refresh') ORDER BY created_at DESC LIMIT 10"#,
        POS_READPOSMOVEMENTSALE_530 => r#"SELECT l.sale_id FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id JOIN pos_sales s ON s.id=l.sale_id WHERE m.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.created_at LIMIT 1"#,
        IMAGES_READPOSCATALOG_531 => r#"SELECT product_id,version FROM pos_catalog WHERE product_id=? AND company_id=?"#,
        IMAGES_READPOSPRODUCTIMAGES_532 => r#"SELECT attachment_source_id FROM pos_product_images WHERE id=? AND company_id=? AND product_id=?"#,
        PMS_READPOSCONNECTIONSPROMPTPAY_533 => r#"SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'"#,
        PMS_READPOSCONNECTIONSCHARGE_534 => r#"SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true) AND (livemode=false OR ?='true')"#,
        PMS_READPOSCONNECTIONSACCOUNT_535 => r#"SELECT account_id,livemode FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true)"#,
        COUNTER_READPOSCONNECTIONREADINESS_536 => r#"SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'"#,
        CORE_READPOSSETTINGSENABLEDCURRENCY_537 => r#"SELECT enabled,currency FROM pos_settings WHERE company_id=?"#,
        CORE_READPOSRESTAURANTREGISTERS_538 => r#"SELECT id,name,warehouse_id,mode FROM pos_registers WHERE company_id=? AND branch_id=? AND active=true AND mode='restaurant'"#,
        CORE_READPOSREGISTERBYID_539 => r#"SELECT * FROM pos_registers WHERE id=?"#,
        STRIPE_READPOSPAYMENTBYIDCOMPANY_540 => r#"SELECT * FROM pos_payments WHERE id=? AND company_id=?"#,
        CORE_READPOSPAYMENTBYID_541 => r#"SELECT * FROM pos_payments WHERE id=?"#,
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
        // Day-report business-date list reads: authoritative owner SQL lives in
        // the dedicated module and is shared with the integration parity oracle.
        kind if crate::adapters::day_report_reads::supported(kind) =>
            return crate::adapters::day_report_reads::sql(kind),
        // Sale-child detail reads keyed by sale id: authoritative owner SQL
        // lives in the dedicated module and is shared with the parity oracle.
        kind if crate::adapters::sale_child_reads::supported(kind) =>
            return crate::adapters::sale_child_reads::sql(kind),
        // Child-tail reads (round 30): authoritative owner SQL lives in the
        // dedicated module and is shared with the transport parity oracle.
        kind if crate::adapters::child_tail_reads::supported(kind) =>
            return crate::adapters::child_tail_reads::sql(kind),
        // List reads (round 31): authoritative owner SQL lives in the dedicated
        // module and is shared with the transport parity oracle.
        kind if crate::adapters::list_reads::supported(kind) =>
            return crate::adapters::list_reads::sql(kind),
        // Hotel day-assertion property reads (round 32): authoritative owner
        // SQL lives in the dedicated module and is shared with the parity oracle.
        kind if crate::adapters::hotel_reads::supported(kind) =>
            return crate::adapters::hotel_reads::sql(kind),
        // Stripe settlement/provider reads: authoritative owner SQL lives in
        // the dedicated module and is shared with the transport parity oracle.
        kind if crate::adapters::stripe_reads::supported(kind) =>
            return crate::adapters::stripe_reads::sql(kind),
        // Audit actor reads: authoritative owner SQL lives in the dedicated
        // module and is shared with the transport parity oracle.
        kind if crate::adapters::audit_reads::supported(kind) =>
            return crate::adapters::audit_reads::sql(kind),
        // Purchase-boundary reads: authoritative owner SQL lives in the
        // dedicated module and is shared with the integration parity oracle.
        kind if crate::adapters::boundary_reads::supported(kind) =>
            return crate::adapters::boundary_reads::sql(kind),
        // POS core workspace reads (shifts family): authoritative owner SQL
        // lives in the dedicated module shared with the integration oracle.
        kind if crate::adapters::core_reads::supported(kind) =>
            return crate::adapters::core_reads::sql(kind),
        // Keyed core settings/receipt/day-close/reversal reads: authoritative
        // owner SQL lives in the dedicated module shared with the parity oracle.
        kind if crate::adapters::core_keyed_reads::supported(kind) =>
            return crate::adapters::core_keyed_reads::sql(kind),
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
        assert_eq!(KINDS.len(), 190);
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

    /// The retired by-company connection row is a SELECT-only owner statement.
    /// Its SQL is byte-identical to the three deleted
    /// `SELECT * FROM pos_connections WHERE company_id=?` literals in
    /// `pos-stripe.ts` (`readPosConnections1`/`3`/`4`); this kind is the sole
    /// authority and every TypeScript copy was removed.
    #[test]
    fn stripe_connections_read_kind_is_select_only() {
        assert!(supported(STRIPE_READPOSCONNECTIONS_526));
        let stmt = sql(STRIPE_READPOSCONNECTIONS_526).unwrap();
        assert!(stmt.starts_with("SELECT "), "connections read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "connections read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "connections read carries only the Company");
        assert_eq!(stmt, "SELECT * FROM pos_connections WHERE company_id=?");
    }

    /// The retired Stripe by-id payment read keeps the caller's row lock. Its
    /// SQL is byte-identical to the four deleted `readPosPayments1`-`4`
    /// `SELECT * FROM pos_payments WHERE id=? FOR UPDATE` literals in
    /// `pos-stripe.ts`; this kind is the sole authority and every TypeScript
    /// copy was removed. `FOR UPDATE` runs on the caller's serializable unit,
    /// so the lock semantics of the retired reads are preserved.
    #[test]
    fn stripe_payment_by_id_read_preserves_for_update() {
        assert!(supported(STRIPE_READPOSPAYMENTSBYID_527));
        let stmt = sql(STRIPE_READPOSPAYMENTSBYID_527).unwrap();
        assert!(stmt.starts_with("SELECT "), "payment read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "payment read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "payment read carries only the payment id");
        assert!(stmt.ends_with(" FOR UPDATE"), "payment read must preserve the FOR UPDATE lock");
        assert_eq!(stmt, "SELECT * FROM pos_payments WHERE id=? FOR UPDATE");
    }

    /// The retired PMS booking connection-readiness read is a SELECT-only owner
    /// statement. Its SQL is byte-identical to the two deleted literals in
    /// `pms-booking-change-facts.ts` and `pms-booking-facts.ts`; this kind is the
    /// sole authority and every TypeScript copy was removed.
    #[test]
    fn booking_connection_read_kind_is_select_only() {
        assert!(supported(STRIPE_READPOSBOOKINGCONNECTION_528));
        let stmt = sql(STRIPE_READPOSBOOKINGCONNECTION_528).unwrap();
        assert!(stmt.starts_with("SELECT "), "booking connection read must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "booking connection read must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "booking connection read carries only the Company");
        assert_eq!(
            stmt,
            "SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true"
        );
    }

    /// The retired Stripe entrypoint jobs read is a SELECT-only owner statement.
    /// Its SQL is byte-identical to the deleted literal in
    /// `stripe-entrypoint-facts.ts`; this kind is the sole authority and every
    /// TypeScript copy was removed. The companion connection read reuses the
    /// byte-identical `stripe_readposconnections_526` kind.
    #[test]
    fn stripe_entrypoint_jobs_read_kind_is_select_only() {
        assert!(supported(STRIPE_READPOSJOBSENTRYPOINT_529));
        let stmt = sql(STRIPE_READPOSJOBSENTRYPOINT_529).unwrap();
        assert!(stmt.starts_with("SELECT "), "entrypoint jobs read kind must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "entrypoint jobs read kind must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1, "entrypoint jobs read carries only the Company");
        assert_eq!(
            stmt,
            "SELECT id,kind,state,last_error,payload_json FROM pos_jobs WHERE company_id=? AND kind IN ('connect','connect_refresh') ORDER BY created_at DESC LIMIT 10"
        );
    }

    /// The retired POS presentation movement-sale read is a read-only owner
    /// statement. Its SQL is byte-identical to the deleted literal in
    /// `movement-presentation-query.ts`; this kind is the sole authority and the
    /// TypeScript copy was removed. Caller-transaction read scoped by movement
    /// id, Company and optional Branch.
    #[test]
    fn pos_movement_sale_read_kind_is_select_only() {
        assert!(supported(POS_READPOSMOVEMENTSALE_530));
        let stmt = sql(POS_READPOSMOVEMENTSALE_530).unwrap();
        assert!(stmt.starts_with("SELECT "), "movement sale read must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "movement sale read must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 4, "movement sale read carries movement, Company and optional Branch");
        assert_eq!(
            stmt,
            "SELECT l.sale_id FROM pos_stock_moves m JOIN pos_lines l ON l.fulfillment_id=m.fulfillment_id JOIN pos_sales s ON s.id=l.sale_id WHERE m.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.created_at LIMIT 1"
        );
    }

    /// The retired product-image catalog guard read is a SELECT-only owner
    /// statement. Its SQL is byte-identical to the deleted literal in
    /// `pos-images.ts`; this kind is the sole authority and the TypeScript copy
    /// was removed. Caller-transaction read scoped by product and Company.
    #[test]
    fn images_catalog_guard_read_kind_is_select_only() {
        assert!(supported(IMAGES_READPOSCATALOG_531));
        let stmt = sql(IMAGES_READPOSCATALOG_531).unwrap();
        assert!(stmt.starts_with("SELECT "), "catalog guard read must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "catalog guard read must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 2, "catalog guard read carries product and Company scope");
        assert_eq!(
            stmt,
            "SELECT product_id,version FROM pos_catalog WHERE product_id=? AND company_id=?"
        );
    }

    /// The retired product-image attachment-source guard read is a SELECT-only
    /// owner statement. Its SQL is byte-identical to the deleted literal in
    /// `pos-images.ts`; this kind is the sole authority and the TypeScript copy
    /// was removed. Caller-transaction read scoped by image, Company and product.
    #[test]
    fn images_product_images_guard_read_kind_is_select_only() {
        assert!(supported(IMAGES_READPOSPRODUCTIMAGES_532));
        let stmt = sql(IMAGES_READPOSPRODUCTIMAGES_532).unwrap();
        assert!(stmt.starts_with("SELECT "), "product-image guard read must be SELECT-only");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";"] {
            assert!(!stmt.contains(forbidden), "product-image guard read must not mutate: {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 3, "product-image guard carries image, Company and product");
        assert_eq!(
            stmt,
            "SELECT attachment_source_id FROM pos_product_images WHERE id=? AND company_id=? AND product_id=?"
        );
    }

    /// The retired PMS/counter-payment connection probes are SELECT-only owner
    /// statements. Their SQL is byte-identical to the literals deleted from
    /// `pms-command-facts.ts` and `pms-counter-payment-facts.ts`; these kinds are
    /// the sole authority and no TypeScript copy, mirror or fallback remains.
    #[test]
    fn pms_and_counter_connection_read_kinds_are_select_only() {
        for kind in [
            PMS_READPOSCONNECTIONSPROMPTPAY_533,
            PMS_READPOSCONNECTIONSCHARGE_534,
            PMS_READPOSCONNECTIONSACCOUNT_535,
            COUNTER_READPOSCONNECTIONREADINESS_536,
        ] {
            assert!(supported(kind), "connection read kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "connection read must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE"] {
                assert!(!stmt.contains(forbidden), "connection read must not mutate or lock: {kind} contains {forbidden}");
            }
        }
        assert_eq!(sql(PMS_READPOSCONNECTIONSPROMPTPAY_533).unwrap().matches('?').count(), 1);
        assert_eq!(sql(PMS_READPOSCONNECTIONSCHARGE_534).unwrap().matches('?').count(), 2);
        assert_eq!(sql(PMS_READPOSCONNECTIONSACCOUNT_535).unwrap().matches('?').count(), 1);
        assert_eq!(sql(COUNTER_READPOSCONNECTIONREADINESS_536).unwrap().matches('?').count(), 1);
        assert_eq!(
            sql(PMS_READPOSCONNECTIONSPROMPTPAY_533).unwrap(),
            "SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'"
        );
        assert_eq!(
            sql(PMS_READPOSCONNECTIONSCHARGE_534).unwrap(),
            "SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true) AND (livemode=false OR ?='true')"
        );
        assert_eq!(
            sql(PMS_READPOSCONNECTIONSACCOUNT_535).unwrap(),
            "SELECT account_id,livemode FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND (card_enabled=true OR promptpay_enabled=true)"
        );
        assert_eq!(
            sql(COUNTER_READPOSCONNECTIONREADINESS_536).unwrap(),
            "SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'"
        );
    }

    /// The retired hospitality-setup settings and restaurant-register reads are
    /// SELECT-only owner statements. Their SQL is byte-identical to the literals
    /// deleted from `pms-hospitality-setup-facts.ts`; these kinds are the sole
    /// authority and no TypeScript copy, mirror or fallback remains. Neither
    /// kind carries a `FOR UPDATE`/`FOR SHARE` lock.
    #[test]
    fn hospitality_setup_read_kinds_are_select_only() {
        for kind in [CORE_READPOSSETTINGSENABLEDCURRENCY_537, CORE_READPOSRESTAURANTREGISTERS_538] {
            assert!(supported(kind), "hospitality-setup read kind must be allow listed: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "hospitality-setup read must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE"] {
                assert!(!stmt.contains(forbidden), "hospitality-setup read must not mutate or lock: {kind} contains {forbidden}");
            }
        }
        assert_eq!(sql(CORE_READPOSSETTINGSENABLEDCURRENCY_537).unwrap().matches('?').count(), 1);
        assert_eq!(sql(CORE_READPOSRESTAURANTREGISTERS_538).unwrap().matches('?').count(), 2);
        assert_eq!(
            sql(CORE_READPOSSETTINGSENABLEDCURRENCY_537).unwrap(),
            "SELECT enabled,currency FROM pos_settings WHERE company_id=?"
        );
        assert_eq!(
            sql(CORE_READPOSRESTAURANTREGISTERS_538).unwrap(),
            "SELECT id,name,warehouse_id,mode FROM pos_registers WHERE company_id=? AND branch_id=? AND active=true AND mode='restaurant'"
        );
    }

    /// The retired by-id register read is a SELECT-only owner statement. Its SQL
    /// is byte-identical to the literals deleted from `pos.ts` and `pos-core.ts`;
    /// this kind is the sole authority and no TypeScript copy, mirror or fallback
    /// remains. The `FOR UPDATE` by-id register reads and refund reads stayed in
    /// TypeScript and are untouched by this kind.
    #[test]
    fn register_by_id_read_kind_is_select_only() {
        let kind = CORE_READPOSREGISTERBYID_539;
        assert!(supported(kind), "by-id register read kind must be allow listed: {kind}");
        let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
        assert!(stmt.starts_with("SELECT "), "by-id register read must be SELECT-only: {kind}");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE"] {
            assert!(!stmt.contains(forbidden), "by-id register read must not mutate or lock: {kind} contains {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1);
        assert_eq!(stmt, "SELECT * FROM pos_registers WHERE id=?");
    }

    /// The retired by-id-and-company payment read is a SELECT-only owner
    /// statement. Its SQL is byte-identical to the two deleted `readPosPayments8`
    /// and `readPosPayments9` literals in `pos-stripe.ts` (checkout flows); this
    /// kind is the sole authority and no TypeScript copy or fallback remains.
    /// The `FOR UPDATE` by-id payment reads and refund reads stayed in
    /// TypeScript and are untouched by this kind.
    #[test]
    fn payment_by_id_company_read_kind_is_select_only() {
        let kind = STRIPE_READPOSPAYMENTBYIDCOMPANY_540;
        assert!(supported(kind), "by-id-and-company payment read kind must be allow listed: {kind}");
        let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
        assert!(stmt.starts_with("SELECT "), "payment read kind must be SELECT-only: {kind}");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE"] {
            assert!(!stmt.contains(forbidden), "payment read kind must not mutate or lock: {kind} contains {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 2);
        assert_eq!(stmt, "SELECT * FROM pos_payments WHERE id=? AND company_id=?");
    }

    /// The retired plain by-id payment read is a SELECT-only owner statement.
    /// Its SQL is byte-identical to the three deleted `readPosPayments6`/
    /// `readPosPayments11`/`readPosPayments16` literals in `pos-stripe.ts` and
    /// `pos-core.ts`; this kind is the sole authority and no TypeScript copy
    /// or fallback remains. It carries no row lock: the `FOR UPDATE` by-id
    /// payment reads and refund reads stay in TypeScript and are untouched.
    #[test]
    fn payment_by_id_plain_read_kind_is_select_only() {
        let kind = CORE_READPOSPAYMENTBYID_541;
        assert!(supported(kind), "plain by-id payment read kind must be allow listed: {kind}");
        let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
        assert!(stmt.starts_with("SELECT "), "payment read kind must be SELECT-only: {kind}");
        for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE"] {
            assert!(!stmt.contains(forbidden), "payment read kind must not mutate or lock: {kind} contains {forbidden}");
        }
        assert_eq!(stmt.matches('?').count(), 1);
        assert_eq!(stmt, "SELECT * FROM pos_payments WHERE id=?");
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

    /// The retired core workspace and keyed register/shift/table reads are
    /// SELECT-only, lock-free owner statements scoped by caller-supplied
    /// Company/branch or key values. No caller SQL is accepted; transport only
    /// selects the statement text.
    #[test]
    fn core_read_kinds_are_select_only() {
        for kind in crate::adapters::core_reads::KINDS {
            assert!(supported(kind), "core read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "core kind must be SELECT-only: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "core kind must not mutate, lock or cross owners: {kind} contains {forbidden}"
                );
            }
        }
        assert_eq!(crate::adapters::core_reads::KINDS.len(), 9);
    }

    /// The keyed core settings/receipt/day-close/reversal reads are SELECT-only,
    /// lock-free owner statements scoped by caller-supplied values. No caller
    /// SQL is accepted; transport only selects the statement text.
    #[test]
    fn keyed_core_read_kinds_are_select_only() {
        for kind in crate::adapters::core_keyed_reads::KINDS {
            assert!(supported(kind), "keyed core read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "keyed core kind must be SELECT-only: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "keyed core kind must not mutate, lock or cross owners: {kind} contains {forbidden}"
                );
            }
        }
        assert_eq!(crate::adapters::core_keyed_reads::KINDS.len(), 8);
    }

    /// The retired day-report list reads are SELECT-only, lock-free owner
    /// statements scoped by caller-supplied Company/branch/business-date values.
    #[test]
    fn day_report_read_kinds_are_select_only() {
        for kind in crate::adapters::day_report_reads::KINDS {
            assert!(supported(kind), "day-report read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "day-report kind must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE", "LOCK TABLE"] {
                assert!(
                    !stmt.contains(forbidden),
                    "day-report kind must not mutate or lock: {kind} contains {forbidden}",
                );
            }
        }
        assert_eq!(crate::adapters::day_report_reads::KINDS.len(), 10);
    }

    /// The retired sale-child detail reads are SELECT-only, lock-free owner
    /// statements scoped by one caller-supplied sale id. No caller SQL is
    /// accepted; transport only selects the statement text.
    #[test]
    fn sale_child_read_kinds_are_select_only() {
        for kind in crate::adapters::sale_child_reads::KINDS {
            assert!(supported(kind), "sale-child read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "sale-child kind must be SELECT-only: {kind}");
            assert!(stmt.contains("sale_id=?"), "sale-child kind must be keyed by sale id: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties"] {
                assert!(
                    !stmt.contains(forbidden),
                    "sale-child kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
        assert_eq!(crate::adapters::sale_child_reads::KINDS.len(), 11);
    }

    /// The retired child-tail reads are SELECT-only, lock-free owner statements
    /// scoped by caller-supplied values. No caller SQL is accepted; transport
    /// only selects the statement text. Row locks, PMS fragments and the
    /// day-report/Stripe registries stay inline in their callers and modules.
    #[test]
    fn child_tail_read_kinds_are_select_only_and_lock_free() {
        for kind in crate::adapters::child_tail_reads::KINDS {
            assert!(supported(kind), "child-tail read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "child-tail kind must be SELECT-only: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties",
                "pos_day_closes", "pos_stripe_routes",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "child-tail kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
        assert_eq!(crate::adapters::child_tail_reads::KINDS.len(), 4);
    }

    /// The retired list reads are SELECT-only, lock-free owner statements
    /// scoped by caller-supplied Company/Branch and id values. No caller SQL is
    /// accepted; transport only selects the statement text. Row locks, PMS
    /// fragments and the day-report/Stripe registries stay inline in their
    /// callers and modules.
    #[test]
    fn list_read_kinds_are_select_only_and_lock_free() {
        for kind in crate::adapters::list_reads::KINDS {
            assert!(supported(kind), "list-read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "list-read kind must be SELECT-only: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties",
                "pos_day_closes", "pos_stripe_routes",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "list-read kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
        assert_eq!(crate::adapters::list_reads::KINDS.len(), 4);
    }

    /// The retired hotel day-assertion property reads are SELECT-only,
    /// Company-scoped, lock-free owner statements on POS-owned tables. The
    /// cross-owner PMS day assertion, row locks and the day-report/Stripe
    /// registries stay inline in their TypeScript callers and modules.
    #[test]
    fn hotel_property_kinds_are_select_only_and_lock_free() {
        for kind in crate::adapters::hotel_reads::KINDS {
            assert!(supported(kind), "hotel property kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "hotel property kind must be SELECT-only: {kind}");
            assert!(stmt.contains("company_id=?"), "hotel property kind must be Company-scoped: {kind}");
            for forbidden in [
                "INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE",
                "FOR NO KEY UPDATE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties",
                "pos_day_closes", "pos_stripe_routes",
            ] {
                assert!(
                    !stmt.contains(forbidden),
                    "hotel property kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
        assert_eq!(crate::adapters::hotel_reads::KINDS.len(), 2);
    }

    /// The retired Stripe settlement/provider reads are SELECT-only, lock-free
    /// owner statements scoped by caller-supplied company/account/livemode and
    /// id values. Row locks, advisory/LOCK, PMS reads and the day-report
    /// registry stay inline in their TypeScript callers and dedicated modules.
    #[test]
    fn stripe_read_kinds_are_select_only_and_lock_free() {
        for kind in crate::adapters::stripe_reads::KINDS {
            assert!(supported(kind), "stripe read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "stripe kind must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties"] {
                assert!(
                    !stmt.contains(forbidden),
                    "stripe kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
        assert_eq!(crate::adapters::stripe_reads::KINDS.len(), 8);
    }

    /// The retired audit actor reads are SELECT-only, lock-free owner
    /// statements on the owner's pos_audit table, scoped by the caller's record
    /// id and a closed operation value. Row locks, advisory/LOCK, PMS reads and
    /// the day-report/Stripe registries stay inline in their callers/modules.
    #[test]
    fn audit_read_kinds_are_select_only_and_lock_free() {
        for kind in crate::adapters::audit_reads::KINDS {
            assert!(supported(kind), "audit read kind must be mounted: {kind}");
            let stmt = sql(kind).unwrap_or_else(|| panic!("missing SQL for {kind}"));
            assert!(stmt.starts_with("SELECT "), "audit kind must be SELECT-only: {kind}");
            for forbidden in ["INSERT", "UPDATE ", "DELETE", "RETURNING", ";", "FOR UPDATE", "FOR SHARE", "LOCK TABLE", "pg_advisory", "pms_sources", "pms_properties", "commerce_"] {
                assert!(
                    !stmt.contains(forbidden),
                    "audit kind must not mutate, lock or cross owners: {kind} contains {forbidden}",
                );
            }
        }
        assert_eq!(crate::adapters::audit_reads::KINDS.len(), 2);
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
