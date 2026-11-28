import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  resolveLane1Target,
  type ExpenseLedgerRow,
  type Lane1Actor,
} from './client';

/**
 * Strict private transport for the lane 1 document posting preview reads. The
 * axum owner executes the three statements and applies the Company scope; this
 * bridge emits no SQL, sends no caller predicate, and accepts only the exact
 * registered envelope. Holding selection is shared with the lane 1 client and
 * is always by WorkContext holdingId + holdingSlug, never a first-holding
 * default. Missing configuration or any native failure fails closed.
 */
export type PostingPreviewLine = { id: string; debit: number; credit: number };
export type PostingPreviewConfiguration = {
  inventory_account_id: string;
  variance_account_id: string;
};

export const DOC_ACCOUNTS = 'doc-accounts';
export const DOC_JOURNAL_LINES = 'doc-journal-lines';
export const DOC_STOCK_CONFIGURATION = 'doc-stock-configuration';

type Kind = typeof DOC_ACCOUNTS | typeof DOC_JOURNAL_LINES | typeof DOC_STOCK_CONFIGURATION;

const unavailable = (code = 'LANE_01_DOC_PREVIEW_UNAVAILABLE'): never => {
  throw new ApiError(503, code, 'The native document posting preview read is unavailable.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 512): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

function validRows(rows: unknown, keys: readonly string[], numeric: readonly string[]): rows is ExpenseLedgerRow[] {
  return Array.isArray(rows) && rows.every(row => {
    if (!record(row) || Object.keys(row).length !== keys.length) return false;
    return keys.every(key => {
      const value = row[key];
      return numeric.includes(key)
        ? typeof value === 'number' && Number.isFinite(value)
        : typeof value === 'string';
    });
  });
}

function validConfiguration(value: unknown): value is PostingPreviewConfiguration {
  return (
    record(value) &&
    Object.keys(value).length === 2 &&
    typeof value.inventory_account_id === 'string' &&
    typeof value.variance_account_id === 'string'
  );
}

async function read(kind: Kind, actor: Lane1Actor, journalId?: string): Promise<unknown> {
  const { config, holding, companyId } = resolveLane1Target(actor);
  if (kind === DOC_JOURNAL_LINES && (typeof journalId !== 'string' || !journalId))
    return unavailable('LANE_01_DOC_PREVIEW_JOURNAL_INVALID');
  if (kind !== DOC_JOURNAL_LINES && journalId !== undefined)
    return unavailable('LANE_01_DOC_PREVIEW_REQUEST_INVALID');

  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/finance/document-posting-preview`,
    config.privateBaseUrl,
  );
  const headers = new Headers({
    host: holding.host,
    origin: holding.origin,
    'content-type': 'application/json',
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
  });
  const body = JSON.stringify(kind === DOC_JOURNAL_LINES ? { kind, journalId } : { kind });
  if (new TextEncoder().encode(body).length > 4096)
    return unavailable('LANE_01_DOC_PREVIEW_INPUT_INVALID');

  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: 'POST',
        headers: Object.fromEntries(headers),
        body,
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const payload = await response.json().catch((): unknown => null);
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(
        response.status,
        text(error?.code, 100) ? String(error!.code) : 'LANE_01_DOC_PREVIEW_READ_FAILED',
        text(error?.message, 300) ? String(error!.message) : 'The native document preview read was rejected.',
      );
    }
    return unavailable();
  }
  const payload = await response.json().catch((): unknown => null);
  const data = record(payload) ? payload.data : undefined;
  if (!record(data) || data.kind !== kind) return unavailable('LANE_01_DOC_PREVIEW_RESPONSE_INVALID');
  if (kind === DOC_ACCOUNTS) {
    if (!validRows(data.rows, ['id', 'code', 'name', 'status'], []))
      return unavailable('LANE_01_DOC_PREVIEW_RESPONSE_INVALID');
  } else if (kind === DOC_JOURNAL_LINES) {
    if (!validRows(data.rows, ['id', 'debit', 'credit'], ['debit', 'credit']))
      return unavailable('LANE_01_DOC_PREVIEW_RESPONSE_INVALID');
  } else if (data.rows !== null && !validConfiguration(data.rows)) {
    return unavailable('LANE_01_DOC_PREVIEW_RESPONSE_INVALID');
  }
  return data.rows;
}

/** Native successor of the document preview `accounts` read. */
export async function documentPostingAccounts(
  _db: SqlDatabase,
  actor: Lane1Actor,
  companyId?: string,
): Promise<ExpenseLedgerRow[]> {
  void _db;
  const { companyId: scoped } = resolveLane1Target(actor);
  // A preview requested for another Company yields no rows, matching the
  // original owner-scoped read; no SQL or business fallback runs here.
  if (companyId !== undefined && companyId !== scoped) return [];
  return (await read(DOC_ACCOUNTS, actor)) as ExpenseLedgerRow[];
}

/** Native successor of the document preview `journalLines` read. */
export async function documentPostingJournalLines(
  _db: SqlDatabase,
  actor: Lane1Actor,
  journalId: string | number,
): Promise<PostingPreviewLine[]> {
  void _db;
  return (await read(DOC_JOURNAL_LINES, actor, String(journalId))) as PostingPreviewLine[];
}

/** Native successor of the document preview `stockConfiguration` read. */
export async function documentPostingStockConfiguration(
  _db: SqlDatabase,
  actor: Lane1Actor,
  companyId?: string,
): Promise<PostingPreviewConfiguration | null> {
  void _db;
  const { companyId: scoped } = resolveLane1Target(actor);
  if (companyId !== undefined && companyId !== scoped) return null;
  return (await read(DOC_STOCK_CONFIGURATION, actor)) as PostingPreviewConfiguration | null;
}
