import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { nativePosOne } from './native-owner-write';

/**
 * Owner-issued POS Stripe settlement/provider reads for caller-owned
 * transactions. The SQL for every kind lives in the native POS owner registry
 * (`erpx-server-pos::adapters::stripe_reads`); this bridge sends only a typed
 * kind and positional company/account/livemode + id values. The caller
 * executes the statement on its own unit, so no TypeScript SQL, mirror or
 * fallback reader remains for this family. Row-lock, advisory/LOCK, PMS
 * cross-owner reads and the day-report registry stay inline in the callers.
 */

const SETTLEMENT_BY_KEY = 'pos_stripe_settlement_by_key';
const SETTLEMENT_BALANCE_STATE = 'pos_stripe_settlement_balance_state';
const SETTLEMENT_PAYOUT_JOURNAL = 'pos_stripe_settlement_payout_journal';
const SETTLEMENT_BALANCE_ROW = 'pos_stripe_settlement_balance_row';
const PAYMENT_BY_ID = 'pos_stripe_payment_by_id';
const PAYMENT_ID = 'pos_stripe_payment_id';
const REFUND_BY_ID = 'pos_stripe_refund_by_id';
const REFUND_ID_BY_PROVIDER = 'pos_stripe_refund_id_by_provider';
const STRIPE_EVENT_BY_ID = 'pos_stripe_event_by_id';
const CONNECTIONS_READY = 'pos_stripe_connections_ready';
const SALE_NUMBER_BY_ID = 'pos_stripe_sale_number_by_id';
const REFUND_PAYMENT_BY_REFUND = 'pos_stripe_refund_payment_by_refund';

type Scope = {
  company_id: string | number | boolean | null;
  account_id: string | number | boolean | null;
  livemode: string | number | boolean | null;
};

/** Balance settlement state by company/account/livemode/balance-transaction key. */
export async function nativePosSettlementStateStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  company_id: string | number | boolean | null;
  account_id: string | number | boolean | null;
  livemode: string | number | boolean | null;
  btId: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', SETTLEMENT_BY_KEY, [f.company_id, f.account_id, f.livemode, f.btId]);
}

/** Balance settlement state by account/livemode/provider id. */
export async function nativePosSettlementBalanceStateStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  account_id: string | number | boolean | null;
  livemode: string | number | boolean | null;
  fact2: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', SETTLEMENT_BALANCE_STATE, [f.account_id, f.livemode, f.fact2]);
}

/** Payout settlement journal by company/account/livemode/provider id. */
export async function nativePosSettlementPayoutJournalStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  company_id: string | number | boolean | null;
  account_id: string | number | boolean | null;
  livemode: string | number | boolean | null;
  id: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', SETTLEMENT_PAYOUT_JOURNAL, [f.company_id, f.account_id, f.livemode, f.id]);
}

/** Balance settlement row by account/livemode/provider id. */
export async function nativePosSettlementBalanceRowStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  account_id: string | number | boolean | null;
  livemode: string | number | boolean | null;
  id: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', SETTLEMENT_BALANCE_ROW, [f.account_id, f.livemode, f.id]);
}

/** Payment by id scoped to Stripe company/account/livemode. */
export async function nativePosStripePaymentStatement<T = Record<string, unknown>>(db: SqlDatabase, f: Scope & {
  pid: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', PAYMENT_BY_ID, [f.pid, f.company_id, f.account_id, f.livemode]);
}

/** Payment id by id scoped to Stripe company/account/livemode. */
export async function nativePosStripePaymentIdStatement<T = Record<string, unknown>>(db: SqlDatabase, f: Scope & {
  paymentId: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', PAYMENT_ID, [f.paymentId, f.company_id, f.account_id, f.livemode]);
}

/** Refund row by id scoped to Stripe company/account/livemode. */
export async function nativePosStripeRefundStatement<T = Record<string, unknown>>(db: SqlDatabase, f: Scope & {
  rpid: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', REFUND_BY_ID, [f.rpid, f.company_id, f.account_id, f.livemode]);
}

/** Refund id by provider source scoped to Stripe company/account/livemode. */
export async function nativePosStripeRefundIdStatement<T = Record<string, unknown>>(db: SqlDatabase, f: Scope & {
  source: string;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', REFUND_ID_BY_PROVIDER, [f.source, f.company_id, f.account_id, f.livemode]);
}

/** Stripe event row by id. */
export async function nativePosStripeEventStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  record_id: string | number | boolean | null;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', STRIPE_EVENT_BY_ID, [f.record_id]);
}

/** Ready Stripe connection by company/account/livemode. */
export async function nativePosStripeConnectionsReadyStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  company_id: string | number | boolean | null;
  accountId: string | undefined;
  fact2: boolean;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', CONNECTIONS_READY, [f.company_id, f.accountId, f.fact2]);
}

/** Sale number by id. */
export async function nativePosStripeSaleNumberStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  sale_id: string | number | boolean | null;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', SALE_NUMBER_BY_ID, [f.sale_id]);
}

/** Stripe refund payment row by id scoped to its refund's Company. */
export async function nativePosStripeRefundPaymentStatement<T = Record<string, unknown>>(db: SqlDatabase, f: {
  record_id: string | number | boolean | null;
  company_id: string | number | boolean | null;
}): Promise<T | null> {
  return nativePosOne<T>(db, '', REFUND_PAYMENT_BY_REFUND, [f.record_id, f.company_id]);
}
