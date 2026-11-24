import 'server-only';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';
import type { SqlValueRow } from '@erpx/server-runtime/public/runtime/sql';
import { nativePosAll, nativePosOne, nativePosStatement } from './native-owner-write';

/**
 * Owner-issued POS core workspace reads (shifts family) for caller-owned
 * transactions. The SQL lives in the native POS owner registry
 * (`erpx-server-pos::adapters::core_reads`); this bridge sends a typed kind and
 * positional values only. The caller executes the statement on its own unit,
 * so no TypeScript SQL, mirror or fallback reader exists for this family.
 */

type Scope = { company: string; branch: string | null; branch2: string | null };

/** Owner-issued shift list with the cash-expected projection. */
export async function nativePosShiftsStatement(db: SqlDatabase, scope: Scope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, 'pos_core_shifts', [scope.company, scope.branch, scope.branch2]);
}

/** Owner-issued restaurant table list. */
export async function nativePosTablesStatement(db: SqlDatabase, scope: Scope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, 'pos_core_tables', [scope.company, scope.branch, scope.branch2]);
}

/** Owner-issued cash-movement list joined to its shift register. */
export async function nativePosCashMovementsStatement(db: SqlDatabase, scope: Scope): Promise<SqlValueRow[]> {
  return nativePosAll(db, scope.company, 'pos_core_cash_movements', [scope.company, scope.branch, scope.branch2]);
}

export type PosSalesEvidenceScope = {
  id: string;
  company: string;
  branch: string | null;
  branch2: string | null;
  currency: string;
  from: string;
  to: string;
};

/**
 * Owner-issued executive-evidence sale row. The SQL lives in the native POS
 * owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossales3_520`); this bridge sends only the typed kind and scoped
 * values. It replaces the two byte-identical `readPosSales3`/`readPosSales4`
 * TypeScript literals in `pos.ts`, so no local SQL, mirror or fallback remains.
 */
export async function nativePosSalesEvidenceStatement(db: SqlDatabase, facts: PosSalesEvidenceScope): Promise<SqlValueRow | null> {
  return nativePosOne(db, facts.company, 'core_readpossales3_520', [
    facts.id, facts.company, facts.branch, facts.branch2, facts.currency, facts.from, facts.to,
  ]);
}

/**
 * Owner-issued plain by-id sale row. The SQL lives in the native POS owner
 * registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossalesbyid_521`); this bridge sends only the typed kind and the
 * sale id. It replaces the three byte-identical by-id `pos_sales` row literals
 * in `pos-core.ts`, `pos-daily.ts` and `pos-stripe.ts`, so no
 * local SQL, mirror or fallback remains. The read is scoped by sale id alone;
 * no actor/Company threading is required.
 */
export async function nativePosSaleByIdStatement(db: SqlDatabase, saleId: unknown): Promise<SqlValueRow | null> {
  return nativePosOne(db, '', 'core_readpossalesbyid_521', [saleId]);
}

/**
 * Owner-issued settings-currency read. The SQL lives in the native POS owner
 * registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossettingscurrency_522`); this bridge sends only the typed kind and
 * the Company value. It replaces the two byte-identical settings-currency
 * row literals in `pos-daily.ts`, so no local SQL, mirror or fallback reader
 * remains.
 */
export async function nativePosSettingsCurrencyStatement(db: SqlDatabase, company: string): Promise<SqlValueRow | null> {
  return nativePosOne(db, company, 'core_readpossettingscurrency_522', [company]);
}

/**
 * Owner-issued hospitality-setup settings read. The SQL lives in the native POS
 * owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossettingsenabledcurrency_537`) and is byte-identical to the
 * settings literal in `pms-hospitality-setup-facts.ts`. Caller-transaction read
 * scoped by Company; no local SQL, mirror or fallback remains.
 */
export async function nativePosSettingsEnabledCurrencyStatement(db: SqlDatabase, company: string): Promise<SqlValueRow | null> {
  return nativePosOne(db, company, 'core_readpossettingsenabledcurrency_537', [company]);
}

/**
 * Owner-issued hospitality-setup restaurant register list. The SQL lives in the
 * native POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readposrestaurantregisters_538`) and is byte-identical to the registers
 * literal in `pms-hospitality-setup-facts.ts`. Caller-transaction read scoped by
 * Company and Branch; no local SQL, mirror or fallback remains.
 */
export async function nativePosRestaurantRegistersStatement(
  db: SqlDatabase,
  company: string,
  branch: string,
): Promise<SqlValueRow[]> {
  return nativePosAll(db, company, 'core_readposrestaurantregisters_538', [company, branch]);
}

/**
 * Owner-issued by-id register row. The SQL lives in the native POS owner
 * registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readposregisterbyid_539`) and is byte-identical to the two retired
 * literals in `pos.ts` (`readPosRegisters3`) and `pos-core.ts`
 * (`readPosRegisters6`). This bridge sends only the typed kind and register id;
 * the caller executes the registry statement on its own transaction, so no
 * local SQL, mirror or fallback remains. The lock-bearing by-id register reads
 * and refund reads stay in TypeScript and are unchanged.
 */
export async function nativePosRegisterByIdStatement<T = SqlValueRow>(
  db: SqlDatabase,
  registerId: unknown,
): Promise<T | null> {
  return nativePosOne<T>(db, '', 'core_readposregisterbyid_539', [registerId]);
}

/**
 * Owner-issued posting-journal id read. The SQL lives in the native POS owner
 * registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpospostingsjournal_523`); this bridge sends only the typed kind and
 * the posting source key. It replaces the two byte-identical journal-id row
 * literals in `pos-core.ts` and `pos-stripe.ts`, so no local SQL, mirror or
 * fallback remains. The read is scoped by source key alone; no actor/Company
 * threading is required.
 */
export async function nativePosPostingJournalStatement(db: SqlDatabase, sourceKey: unknown): Promise<SqlValueRow | null> {
  return nativePosOne(db, '', 'core_readpospostingsjournal_523', [sourceKey]);
}

/**
 * Owner-issued register-catalog membership read. The SQL lives in the native
 * POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readposregistercatalog_524`); this bridge sends only the typed kind and
 * the product, branch and Company values. It replaces the two byte-identical
 * `readPosRegisterCatalog1` SQL literals in `pos-core.ts` and `pos-images.ts`,
 * so no local SQL, mirror or fallback remains. Caller-transaction read scoped
 * by product, branch and Company.
 */
export async function nativePosRegisterCatalogStatement(
  db: SqlDatabase,
  productId: string,
  branchId: string,
  company: string,
): Promise<SqlValueRow | null> {
  return nativePosOne(db, company, 'core_readposregistercatalog_524', [productId, branchId, company]);
}

/**
 * Owner-issued POS settings accounts read. The SQL lives in the native POS
 * owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpossettingsaccounts_525`); this bridge sends only the typed kind and
 * the Company value. It replaces the three byte-identical accounts-json row
 * literals in `pms-booking-change-facts.ts`, `pms-booking-facts.ts` and
 * `pms-counter-payment-facts.ts`, so no local SQL, mirror or fallback reader
 * remains. Caller-transaction read scoped by Company.
 */
export async function nativePosSettingsAccountsStatement<T = SqlValueRow>(
  db: SqlDatabase,
  company: string,
): Promise<T | null> {
  return nativePosOne<T>(db, company, 'core_readpossettingsaccounts_525', [company]);
}

/**
 * Owner-issued Stripe control-route read. The SQL lives in the native POS
 * owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `stripe_readposstriperoutes_355`); this bridge sends only the typed kind and
 * positional values. It replaces the three byte-identical
 * `pos_stripe_routes` account/livemode row literals in `pos-stripe.ts`, so no
 * local SQL, mirror or fallback reader remains.
 * Caller-transaction read scoped by account id and livemode.
 */
export async function nativePosStripeRoutesStatement<T = SqlValueRow>(
  db: SqlDatabase,
  accountId: string,
  livemode: boolean,
): Promise<T | null> {
  return nativePosOne<T>(db, '', 'stripe_readposstriperoutes_355', [accountId, livemode]);
}

/**
 * Owner-issued Stripe connection row read. The SQL lives in the native POS
 * owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `stripe_readposconnections_526`); this bridge sends only the typed kind and
 * the Company value. It replaces the three byte-identical `pos_connections`
 * by-company row literals (`readPosConnections1`/`3`/`4`) in `pos-stripe.ts`, so
 * no local SQL, mirror or fallback reader remains. Caller-transaction read
 * scoped by Company.
 */
export async function nativePosConnectionsStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: unknown,
): Promise<T | null> {
  return nativePosOne<T>(db, String(companyId ?? ''), 'stripe_readposconnections_526', [companyId]);
}

/**
 * Owner-issued Stripe by-id payment row with the caller's row lock. The SQL lives
 * in the native POS owner registry (`erpx-server-pos::adapters::owner_writes`,
 * kind `stripe_readpospaymentsbyid_527`) and is byte-identical to the four
 * deleted by-id payment row literals in `pos-stripe.ts`
 * (`readPosPayments1`-`4`). The registry statement carries
 * `FOR UPDATE`, which this bridge binds and executes on the caller's own
 * serializable unit, so the Stripe transaction lock semantics are unchanged.
 * No TypeScript SQL, mirror or fallback reader remains for this family.
 */
export async function nativePosPaymentByIdStatement<T = SqlValueRow>(
  db: SqlDatabase,
  paymentId: unknown,
): Promise<T | null> {
  return nativePosOne<T>(db, '', 'stripe_readpospaymentsbyid_527', [paymentId]);
}

/**
 * Owner-issued PMS booking connection-readiness row. The SQL lives in the
 * native POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `stripe_readposbookingconnection_528`) and is byte-identical to the two
 * deleted `SELECT * FROM pos_connections WHERE company_id=? AND state='ready'
 * AND charges_enabled=true AND payouts_enabled=true` literals in
 * `pms-booking-change-facts.ts` and `pms-booking-facts.ts`. This bridge sends
 * only the kind and Company value; the caller executes the registry statement
 * on its own unit, so no TypeScript SQL, mirror or fallback reader remains.
 */
export async function nativePosBookingConnectionStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: unknown,
): Promise<T | null> {
  return nativePosOne<T>(db, String(companyId ?? ''), 'stripe_readposbookingconnection_528', [companyId]);
}

/**
 * Owner-issued Stripe entrypoint connection-readiness jobs read. The SQL lives
 * in the native POS owner registry (`erpx-server-pos::adapters::owner_writes`,
 * kind `stripe_readposjobsentrypoint_529`) and is byte-identical to the deleted
 * literal in `stripe-entrypoint-facts.ts`. This bridge sends only the kind and
 * Company value; the caller executes the registry statement on its own unit,
 * so no TypeScript SQL, mirror or fallback reader remains. The companion
 * connection read reuses the byte-identical `nativePosConnectionsStatement`
 * (`stripe_readposconnections_526`). The full `SqlResult` is returned so the
 * presentation consumer keeps its existing `.results` contract unchanged.
 */
export async function nativePosStripeEntrypointJobsStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: unknown,
): Promise<SqlResult<T>> {
  return (await nativePosStatement(db, String(companyId ?? ''), 'stripe_readposjobsentrypoint_529', [companyId])).all<T>();
}

/**
 * Owner-issued POS presentation movement-sale projection. The read lives in
 * the native POS owner registry (`erpx-server-pos::adapters::owner_writes`,
 * kind `pos_readposmovementsale_530`) and is byte-identical to the retired
 * literal in `movement-presentation-query.ts`. This bridge sends only the kind
 * and the movement/Company/Branch values; the caller executes the registry
 * statement on its own unit, so no local SQL, mirror or fallback reader remains.
 */
export async function nativePosMovementSaleStatement<T = SqlValueRow>(
  db: SqlDatabase,
  movementId: string,
  companyId: string,
  branchId: string | null,
): Promise<T | null> {
  return nativePosOne<T>(db, companyId, 'pos_readposmovementsale_530', [movementId, companyId, branchId, branchId]);
}

/**
 * Owner-issued POS product-image catalog guard read. The SQL lives in the
 * native POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `images_readposcatalog_531`) and is byte-identical to the retired guard
 * literal in `pos-images.ts`, which validated the current catalog version
 * before saving a product image. This bridge sends only the kind and the
 * product/Company values; the caller executes the registry statement on its
 * own unit, so no local SQL, mirror or fallback reader remains.
 */
export async function nativePosCatalogStatement<T = SqlValueRow>(
  db: SqlDatabase,
  productId: string,
  company: string,
): Promise<T | null> {
  return nativePosOne<T>(db, company, 'images_readposcatalog_531', [productId, company]);
}

/**
 * Owner-issued POS product-image attachment-source guard read. The SQL lives
 * in the native POS owner registry (`erpx-server-pos::adapters::owner_writes`,
 * kind `images_readposproductimages_532`) and is byte-identical to the retired
 * guard literal in `pos-images.ts`, which read the attachment source id of an
 * existing product image before a versioned save. This bridge sends only the
 * kind and the image/Company/product values; the caller executes the registry
 * statement on its own unit, so no local SQL, mirror or fallback reader
 * remains.
 */
export async function nativePosProductImagesStatement<T = SqlValueRow>(
  db: SqlDatabase,
  imageId: string,
  company: string,
  productId: string,
): Promise<T | null> {
  return nativePosOne<T>(db, company, 'images_readposproductimages_532', [imageId, company, productId]);
}

/**
 * Owner-issued PMS PromptPay/daily-payout connection probe. The SQL lives in
 * the native POS owner registry (`erpx-server-pos::adapters::owner_writes`,
 * kind `pms_readposconnectionspromptpay_533`) and is byte-identical to the
 * retired `readPosConnections` literal in `pms-command-facts.ts`. This bridge
 * sends only the kind and Company value; the caller executes the registry
 * statement on its own unit, so no local SQL, mirror or fallback remains.
 */
export async function nativePosPromptPayConnectionStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: string,
): Promise<T | null> {
  return nativePosOne<T>(db, companyId, 'pms_readposconnectionspromptpay_533', [companyId]);
}

/**
 * Owner-issued PMS card/PromptPay charge-readiness probe. The SQL lives in the
 * native POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `pms_readposconnectionscharge_534`) and is byte-identical to the retired
 * `readPosConnections2` literal in `pms-command-facts.ts`. This bridge sends
 * only the kind, Company and livemode-fact values; the caller executes the
 * registry statement on its own unit, so no local SQL, mirror or fallback
 * remains.
 */
export async function nativePosChargeConnectionStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: string,
  fact1: string,
): Promise<T | null> {
  return nativePosOne<T>(db, companyId, 'pms_readposconnectionscharge_534', [companyId, fact1]);
}

/**
 * Owner-issued PMS connection account/livemode probe. The SQL lives in the
 * native POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `pms_readposconnectionsaccount_535`) and is byte-identical to the retired
 * `readPosConnections3` literal in `pms-command-facts.ts`. This bridge sends
 * only the kind and Company value; the caller executes the registry statement
 * on its own unit, so no local SQL, mirror or fallback remains.
 */
export async function nativePosConnectionAccountStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: string,
): Promise<T | null> {
  return nativePosOne<T>(db, companyId, 'pms_readposconnectionsaccount_535', [companyId]);
}

/**
 * Owner-issued counter-payment connection readiness read. The SQL lives in
 * the native POS owner registry (`erpx-server-pos::adapters::owner_writes`,
 * kind `counter_readposconnectionreadiness_536`) and is byte-identical to the
 * retired `readPosConnections` literal in `pms-counter-payment-facts.ts`. This
 * bridge sends only the kind and Company value; the caller executes the
 * registry statement on its own unit, so no local SQL, mirror or fallback
 * remains.
 */
export async function nativePosCounterPaymentConnectionStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: string,
): Promise<T | null> {
  return nativePosOne<T>(db, companyId, 'counter_readposconnectionreadiness_536', [companyId]);
}

/**
 * Owner-issued by-id-and-company payment row read. The SQL lives in the native
 * POS owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `stripe_readpospaymentbyidcompany_540`) and is byte-identical to the two
 * retired `readPosPayments8`/`readPosPayments9` literals in `pos-stripe.ts`
 * (checkout flows). This bridge sends only the typed kind and the id/company
 * values; the caller executes the registry statement on its own unit, so no
 * local SQL, mirror or fallback reader remains. The `FOR UPDATE` by-id payment
 * reads and refund reads stay in TypeScript and are unchanged.
 */
export async function nativePosPaymentByIdCompanyStatement<T = SqlValueRow>(
  db: SqlDatabase,
  companyId: unknown,
  paymentId: unknown,
): Promise<T | null> {
  return nativePosOne<T>(db, String(companyId), 'stripe_readpospaymentbyidcompany_540', [paymentId, companyId]);
}

/**
 * Owner-issued plain by-id payment row read. The SQL lives in the native POS
 * owner registry (`erpx-server-pos::adapters::owner_writes`, kind
 * `core_readpospaymentbyid_541`) and is byte-identical to the three retired
 * by-id `pos_payments` row literals in `pos-stripe.ts`
 * (`readPosPayments6`/`readPosPayments11`) and `pos-core.ts`
 * (`readPosPayments16`). This bridge sends only the typed kind and the payment
 * id; the caller executes the registry statement on its own unit, so no local
 * SQL, mirror or fallback reader remains. The `FOR UPDATE` by-id payment reads
 * and refund reads stay in TypeScript and are unchanged.
 */
export async function nativePosPaymentByIdPlainStatement<T = SqlValueRow>(
  db: SqlDatabase,
  paymentId: unknown,
): Promise<T | null> {
  return nativePosOne<T>(db, '', 'core_readpospaymentbyid_541', [paymentId]);
}
