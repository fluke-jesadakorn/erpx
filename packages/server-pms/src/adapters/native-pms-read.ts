import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase, SqlResult } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict owner transport for the mounted PMS `command-plan` read route.
 *
 * The axum owner plans the exact statement for a registered read kind; this
 * bridge emits no SQL, accepts no SQL from a caller, and executes the returned
 * statement on the caller's already-open unit so the read observes uncommitted
 * effects in the caller's serializable transaction. Read kinds carry no
 * receipt/audit/outbox ledger. Missing configuration or any configured failure
 * fails closed with no TypeScript fallback.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** Read-kind ids shared with the Rust read registry in `native_read_sql.rs`. */
export const PMS_RD_INVENTORY_COMMAND = 'pms_rd_inventory_command';
export const PMS_RD_BOOKING_CHANGE = 'pms_rd_booking_change';
export const PMS_RD_BOOKING_CHANGE_PENDING = 'pms_rd_booking_change_pending';
export const PMS_RD_BOOKING_PAYMENT = 'pms_rd_booking_payment';
export const PMS_RD_POSTING_CHARGE = 'pms_rd_posting_charge';
export const PMS_RD_POSTING_BY_RECORD = 'pms_rd_posting_by_record';
export const PMS_RD_CHARGE_CORRECTION_PENDING = 'pms_rd_charge_correction_pending';
export const PMS_RD_CHARGE_CORRECTION_PENDING_COUNT = 'pms_rd_charge_correction_pending_count';
export const PMS_RD_GUEST_COMMAND_RECORD = 'pms_rd_guest_command_record';
export const PMS_RD_DOCUMENT_BY_ID = 'pms_rd_document_by_id';
export const PMS_RD_DOCUMENT_SNAPSHOT = 'pms_rd_document_snapshot';
export const PMS_RD_PROPERTY_CONFIG = 'pms_rd_property_config';
export const PMS_RD_PENDING_HOTEL_TRANSFERS = 'pms_rd_pending_hotel_transfers';
export const PMS_RD_UNPOSTED_BOOKING_RECEIPTS = 'pms_rd_unposted_booking_receipts';
export const PMS_RD_UNPOSTED_COUNTER_RECEIPTS = 'pms_rd_unposted_counter_receipts';
export const PMS_RD_HOSPITALITY_SETUP_PROPERTIES = 'pms_rd_hospitality_setup_properties';
export const PMS_RD_ATTACHMENT_PROPERTY_IMAGE = 'pms_rd_attachment_property_image';
export const PMS_RD_ATTACHMENT_RECORD_IMAGE = 'pms_rd_attachment_record_image';
export const PMS_RD_DOCUMENT_RELATED_POSTINGS = 'pms_rd_document_related_postings';
export const PMS_RD_POS_GUEST_HOTELS = 'pms_rd_pos_guest_hotels';
export const PMS_RD_POS_GUEST_CHOICES = 'pms_rd_pos_guest_choices';
/** Native successors of the owner-local `posStripePmsFacts` byte-identical
 * point reads: counter/booking payment existence checks. Fixed SQL with four
 * positional binds, no ledger/receipt/outbox and no lock/refund path; the
 * caller runs them on its already-open read unit. Dedicated refund-state and
 * FOR UPDATE reads stay in the caller's TypeScript unit. */
export const PMS_RD_POS_STRIPE_PAYMENT = 'pms_rd_pos_stripe_payment';
export const PMS_RD_POS_BOOKING_PAYMENT = 'pms_rd_pos_booking_payment';
/** Native successors of the owner-local `pms-stripe.ts` byte-identical plain
 * payment point reads. Fixed SQL with one/two positional binds, no
 * ledger/receipt/outbox and no lock/refund path; the caller runs them on its
 * already-open read unit. FOR UPDATE and refund-state reads stay in TS. */
export const PMS_RD_STRIPE_PAYMENT_BY_ID = 'pms_rd_stripe_payment_by_id';
export const PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY = 'pms_rd_stripe_payment_by_id_company';
export const PMS_RD_STRIPE_PAYMENT_PROPERTY = 'pms_rd_stripe_payment_property';
/** Native successors of the owner-local scoped-records command readers. The
 * retired template literals are expanded byte-identically in the Rust registry;
 * six/five/three/three positional binds and no lock, refund or ledger path. */
export const PMS_RD_STAY_COMMAND = 'pms_rd_stay_command';
export const PMS_RD_CASHIER_COMMAND = 'pms_rd_cashier_command';
export const PMS_RD_POS_COMMAND = 'pms_rd_pos_command';
export const PMS_RD_NIGHT_COMMAND = 'pms_rd_night_command';
/** Native successors of the owner-local `hotelSources` recursive-CTE read (eleven
 * positional binds) and the housekeeping command snapshot read (four positional
 * binds). Both are non-lock reads: the caller runs them on its already-open unit
 * with the same payload, so a caller holding the property `FOR UPDATE` observes
 * its own uncommitted state. The property lock read itself stays in the caller. */
export const PMS_RD_HOTEL_SOURCES = 'pms_rd_hotel_sources';
export const PMS_RD_HOUSEKEEPING_COMMAND = 'pms_rd_housekeeping_command';
/** Native successor of the owner-local `sectionRelatedRecords` section-list read. The
 * retired 17-token TypeScript template is expanded byte-identically in the Rust
 * registry with 29 positional binds; no ledger/receipt/outbox and no lock or refund
 * path. The caller runs it on its already-open unit. */
export const PMS_RD_SECTION_RELATED_RECORDS = 'pms_rd_section_related_records';
/** Native successor of the owner-local `pms/guest-document.ts` point read. Fixed
 * SQL with three positional binds, no ledger/receipt/outbox and no lock/refund
 * path; the guest booking document transaction runs it on its already-open unit. */
export const PMS_RD_GUEST_BOOKING_DOCUMENT = 'pms_rd_guest_booking_document';
/** Native successor of the owner-local `hotelMetrics` property-scope read. The
 * exact retained statement is planned in Rust with five positional binds; the
 * caller runs it on its already-open unit with the same Company/branch scope.
 * No TypeScript fallback and no money/KPI claim is made by this kind. */
export const PMS_RD_HOTEL_METRICS = 'pms_rd_hotel_metrics';
/** Native successors of the owner-local `pms-booking.ts` receipt point reads.
 * Fixed SQL with one/two positional binds, no ledger/receipt/outbox and no
 * lock/refund path; the caller runs them on its already-open booking unit. */
export const PMS_RD_BOOKING_RECEIPT_BY_KEY = 'pms_rd_booking_receipt_by_key';
export const PMS_RD_BOOKING_RECEIPT_BY_ATTEMPT = 'pms_rd_booking_receipt_by_attempt';
/** Native successors of the owner-local booking companion reads used when a
 * management token opens a reservation: guest-document list and pending
 * change-payment point read. Fixed SQL with three/one positional binds, no
 * ledger/receipt/outbox and no lock/refund path; caller runs them on its unit. */
export const PMS_RD_BOOKING_DOCUMENTS = 'pms_rd_booking_documents';
export const PMS_RD_CHANGE_PAYMENT_PENDING = 'pms_rd_change_payment_pending';
/** Native successors of the paired refund-allocation companion reads. Fixed
 * SQL with one positional bind each, no ledger/receipt/outbox and no
 * lock/refund decision; the caller runs them on its already-open unit. */
export const PMS_RD_BOOKING_ALLOCATIONS = 'pms_rd_booking_allocations';
export const PMS_RD_REFUND_REQUEST_ALLOCATIONS = 'pms_rd_refund_request_allocations';
/** Native successors of the owner-local `pms.ts` transfer/document companion
 * reads. Fixed SQL with two positional binds each, no ledger/receipt/outbox
 * and no lock/refund path; the caller runs them on its already-open unit. */
export const PMS_RD_TRANSFER_REQUESTS = 'pms_rd_transfer_requests';
export const PMS_RD_GUEST_DOCUMENTS = 'pms_rd_guest_documents';
/** Native successors of the owner-local `pms.ts` fixed allocation-existence
 * and pending-job list reads. Fixed SQL with two/four positional binds, no
 * ledger/receipt/outbox and no lock/refund path; caller runs them on its unit. */
export const PMS_RD_BOOKING_ALLOCATION_EXISTS = 'pms_rd_booking_allocation_exists';
export const PMS_RD_PENDING_JOBS = 'pms_rd_pending_jobs';
/** Native successors of the owner-local `pms-booking.ts` property-scope and
 * records-list reads, including their closed branch/search variants. Fixed
 * SQL with 2/3 and 1/3 positional binds, no ledger/receipt/outbox and no
 * lock/refund path; the caller runs them on its already-open booking unit. */
export const PMS_RD_BOOKING_PROPERTY_SCOPE = 'pms_rd_booking_property_scope';
export const PMS_RD_BOOKING_PROPERTY_SCOPE_BRANCH = 'pms_rd_booking_property_scope_branch';
export const PMS_RD_BOOKING_RECORDS = 'pms_rd_booking_records';
export const PMS_RD_BOOKING_RECORDS_SEARCH = 'pms_rd_booking_records_search';
/** Native successors of the owner-local `pms.ts` guest-account sum and
 * charge-correction reads (with the closed front-mode variant). Fixed SQL with
 * one/three and two positional binds, no ledger/receipt/outbox and no
 * lock/refund path; the caller runs them on its already-open unit. */
export const PMS_RD_GUEST_ACCOUNT_SUMS = 'pms_rd_guest_account_sums';
export const PMS_RD_CHARGE_CORRECTIONS = 'pms_rd_charge_corrections';
export const PMS_RD_CHARGE_CORRECTIONS_FRONT = 'pms_rd_charge_corrections_front';

const unavailable = (): never => {
  throw new ApiError(503, 'PMS_NATIVE_READ_UNAVAILABLE', 'The native PMS read service is unavailable.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const identifier = (v: unknown): v is string =>
  typeof v === 'string' && v.length > 0 && v.length <= 200 &&
  // eslint-disable-next-line no-control-regex
  !/[\u0000-\u001f/?#\\]/.test(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_PMS_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' ||
        typeof item.holdingId !== 'string' || !identifier(item.holdingId) ||
        !/^[a-z0-9_-]+$/.test(item.holdingSlug)) return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Plan an owner read in Rust and run it on the caller's open unit. Pure read:
 * the kind selects the SQL; only routing-scoped values cross the wire. */
export async function nativePmsReadAll<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<SqlResult<T>> {
  const config = configuration();
  const holding = config.holdings[0]!;
  const key = 'pms-read-' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex').slice(0, 32);
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/pms/command-plan`, config.privateBaseUrl);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: {
        host: new URL(holding.origin).host,
        origin: holding.origin,
        'content-type': 'application/json',
        accept: 'application/json',
        'cache-control': 'no-cache',
        'idempotency-key': key,
      },
      body: JSON.stringify({ kind, values }),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = await response.json().catch((): unknown => null);
    const err = record(payload) ? (payload as { error?: { code?: string; message?: string } }).error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(response.status, err?.code ?? 'PMS_NATIVE_READ_FAILED', err?.message ?? 'The native PMS read was rejected.');
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const statements = record(payload) ? (payload as { data?: { statements?: unknown } }).data?.statements : undefined;
  if (!Array.isArray(statements) || statements.length !== 1) return unavailable();
  const planned = statements.map((statement: unknown) => {
    if (!record(statement) || typeof statement.sql !== 'string' || !Array.isArray(statement.values)) return unavailable();
    return db.prepare(statement.sql).bind(...statement.values);
  });
  const results = await db.batch<T>(planned);
  return results[0] ?? { results: [], success: true, meta: { changes: 0, duration: 0 } };
}

/** Run a single-row owner read and return its first row (or null). */
export async function nativePmsReadFirst<T>(
  kind: string,
  values: readonly unknown[],
  db: SqlDatabase,
): Promise<T | null> {
  const result = await nativePmsReadAll<T>(kind, values, db);
  return result.results[0] ?? null;
}
