import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFile } from 'node:fs/promises';
vi.mock('server-only', () => ({}));
import { ACTIVE_CHART_BASE_CURRENCY_SQL } from '../../../packages/server-finance/src/adapters/chart-base-currency-dedupe-sql';
import { FINANCE_JOURNALS_PROJECTION_SQL } from '../../../packages/server-finance/src/adapters/finance-journals-projection-dedupe-sql';
import { FINANCE_CHARTS_PROJECTION_SQL } from '../../../packages/server-finance/src/adapters/finance-charts-projection-dedupe-sql';
import { finalRootOwnerProjectionSources } from '../../../packages/server-finance/src/adapters/final-root-owner-projection-sources';
import { platformOwnerProjectionSources } from '../../../packages/server-finance/src/adapters/platform-owner-projection-sources';
import { inventoryExecutiveOwnerProjections } from '../../../packages/server-finance/src/adapters/inventory-executive-owner-projections';
import { commercialOwnerProjectionSources } from '../../../packages/server-finance/src/adapters/commercial-owner-projection-sources';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-finance/src/adapters/hospitality-owner-projection-sources';
import {
  POSTING_FINANCE_PERIODS_SQL,
  POSTING_COMPANY_FX_RATES_SQL,
  POSTING_COMPANY_MONEY_IMAGE_SQL,
  POSTING_COMMAND_RECEIPT_RECORD_SQL,
} from '../../../packages/server-finance/src/adapters/posting-prerequisites-dedupe-sql';
import { FINANCE_ADVISORY_XACT_LOCK_SQL } from '../../../packages/server-finance/src/adapters/advisory-lock-dedupe-sql';
import { FINANCE_RECEIPT_IMAGE_SQL, FINANCE_INVOICES_PROJECTION_SQL, FINANCE_PARTIES_PROJECTION_SQL, FINANCE_ACCOUNTS_PROJECTION_SQL } from '../../../packages/server-finance/src/adapters/finance-tail-dedupe-sql';
import {
  CHART_BASE_CURRENCY_BY_ID_SQL,
  OPEN_PERIOD_LIST_SQL,
  APPROVED_SECONDARY_CHART_RULE_SQL,
  FINANCE_JOURNALS_JOIN_SQL,
  COMMAND_RECEIPT_EXISTS_SQL,
  COMMAND_RECEIPT_RECORD_IMAGE_SQL,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r15-sql';
import { OPEN_PERIOD_GUARD_SQL, OPEN_PERIOD_IDS_SQL } from '../../../packages/server-finance/src/adapters/finance-read-tail-r16-sql';
import { SUPPLIER_RESERVATION_GUARD_SQL, BANK_TRANSACTION_BY_ID_SQL, DIMENSION_DEFINITIONS_SQL } from '../../../packages/server-finance/src/adapters/finance-read-tail-r17-sql';
import {
  MONEY_SETTINGS_IMAGE_SQL, MONEY_TXN_SNAPSHOT_SOURCE_SQL,
  PAYMENT_PROJECTION_SQL, REFUND_OBLIGATION_PROJECTION_SQL,
  ACCOUNT_CHART_ID_SQL, ALLOCATION_PROJECTION_SQL,
  PERIOD_DATES_SQL, COLLECTIONS_TOOL_GUARD_SQL, BANK_STATEMENT_VERSION_GUARD_SQL,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r18-sql';
import { rootOwnerExistsSql, materializedClockCteSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r19-sql';
import { idCompanyStatusVersionGuard, aggByCompany } from '../../../packages/server-finance/src/adapters/finance-read-tail-r20-sql';
import { journalLineEvidenceSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r22-sql';
import { peoplePayrollFinalOwnerProjections } from '../../../packages/server-finance/src/adapters/people-payroll-final-owner-projections';
import { workflowFinalOwnerProjections } from '../../../packages/server-finance/src/adapters/workflow-final-owner-projections';
import {
  nativeFinanceReadAll,
  nativeFinanceReadFirst,
  FINANCE_RD_ASSET_POLICY_ROW,
  FINANCE_RD_ASSET_POLICY_RUNS,
  FINANCE_RD_INSTRUMENT_EDITION,
  FINANCE_RD_INSTRUMENT_RUNS,
  FINANCE_RD_COLLECTION_CASE_ROW,
  FINANCE_RD_BUSINESS_BILL_ROW,
} from '../../../packages/server-finance/src/adapters/native-finance-read';

const envKey = 'ERPX_NATIVE_FINANCE_READ_CONFIG';
const original = process.env[envKey];
beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44992',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
  });
});
afterEach(() => {
  if (original === undefined) delete process.env[envKey];
  else process.env[envKey] = original;
  vi.unstubAllGlobals();
});

const fakeDb = () => {
  const bound: unknown[][] = [];
  const prepared: string[] = [];
  const db = {
    prepare(sql: string) {
      prepared.push(sql);
      return {
        bind(...values: unknown[]) { bound.push(values); return this; },
        async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
        async first() { return null; },
      };
    },
    async batch() { return [{ results: [{ id: 'row-1' }], success: true, meta: { changes: 0, duration: 0 } }]; },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared };
};

describe('native Finance caller-unit read plan', () => {
  it('exposes an allow-listed FINANCE_RD_* registry with no SQL on the wire', () => {
    expect(FINANCE_RD_ASSET_POLICY_ROW).toBe('finance_rd_asset_policy_row');
    expect(FINANCE_RD_ASSET_POLICY_RUNS).toBe('finance_rd_asset_policy_runs');
    expect(FINANCE_RD_COLLECTION_CASE_ROW).toBe('finance_rd_collection_case_row');
    expect(FINANCE_RD_BUSINESS_BILL_ROW).toBe('finance_rd_business_bill_row');
    for (const kind of [
      FINANCE_RD_ASSET_POLICY_ROW,
      FINANCE_RD_ASSET_POLICY_RUNS,
      FINANCE_RD_INSTRUMENT_EDITION,
      FINANCE_RD_INSTRUMENT_RUNS,
      FINANCE_RD_COLLECTION_CASE_ROW,
      FINANCE_RD_BUSINESS_BILL_ROW,
    ]) expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans a registered read and runs it on the caller unit with its binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    expect(await nativeFinanceReadFirst(FINANCE_RD_COLLECTION_CASE_ROW, ['company', 'case-1'], db as never)).toEqual({ id: 'row-1' });
    expect(await nativeFinanceReadAll(FINANCE_RD_ASSET_POLICY_RUNS, ['company', 'policy-1'], db as never)).toMatchObject({ results: [{ id: 'row-1' }] });

    expect(bodies.map((b) => b.kind)).toEqual([FINANCE_RD_COLLECTION_CASE_ROW, FINANCE_RD_ASSET_POLICY_RUNS]);
    expect(bodies.map((b) => b.values)).toEqual([['company', 'case-1'], ['company', 'policy-1']]);
    expect(bound).toEqual([['company', 'case-1'], ['company', 'policy-1']]);
    expect(prepared).toEqual(['SELECT 1 AS row', 'SELECT 1 AS row']);

    const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/finance/command-plan');
    expect((init.headers as Record<string, string>).origin).toBe('https://acme.example.com');
    expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe('company');
  });

  it('plans the instrument fixed reads in Rust and runs them on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push(body);
      const sql = body.kind === FINANCE_RD_INSTRUMENT_EDITION
        ? 'SELECT * FROM finance_instrument_editions WHERE company_id=? AND id=?'
        : 'SELECT * FROM finance_instrument_runs WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id';
      return new Response(JSON.stringify({ data: { statements: [{ sql, values: body.values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
    const { db, bound, prepared } = fakeDb();

    expect(await nativeFinanceReadFirst(FINANCE_RD_INSTRUMENT_EDITION, ['company', 'edition'], db as never)).toEqual({ id: 'row-1' });
    expect(await nativeFinanceReadAll(FINANCE_RD_INSTRUMENT_RUNS, ['company', 'edition'], db as never)).toMatchObject({ results: [{ id: 'row-1' }] });

    expect(bodies.map((b) => b.kind)).toEqual([FINANCE_RD_INSTRUMENT_EDITION, FINANCE_RD_INSTRUMENT_RUNS]);
    expect(bodies.map((b) => b.values)).toEqual([['company', 'edition'], ['company', 'edition']]);
    expect(bound).toEqual([['company', 'edition'], ['company', 'edition']]);
    expect(prepared).toEqual([
      'SELECT * FROM finance_instrument_editions WHERE company_id=? AND id=?',
      'SELECT * FROM finance_instrument_runs WHERE company_id=? AND instrument_id=? ORDER BY ordinal,id',
    ]);
  });

  it('fails closed when configuration is missing or the plan is malformed', async () => {
    delete process.env[envKey];
    const { db } = fakeDb();
    await expect(nativeFinanceReadFirst(FINANCE_RD_ASSET_POLICY_ROW, ['company', 'p'], db as never))
      .rejects.toMatchObject({ code: 'FINANCE_NATIVE_READ_UNAVAILABLE' });

    process.env[envKey] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44992', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: {} }), {
      status: 200, headers: { 'content-type': 'application/json' },
    })));
    await expect(nativeFinanceReadFirst(FINANCE_RD_ASSET_POLICY_ROW, ['company', 'p'], db as never))
      .rejects.toMatchObject({ code: 'FINANCE_NATIVE_READ_UNAVAILABLE' });
  });
});

const CHART_SQL = "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'";
const CHART_CALL_SITES = [
  'packages/server-finance/src/adapters/finance-planning.ts',
  'packages/server-finance/src/adapters/accounting-profiles.ts',
  'packages/server-finance/src/adapters/payroll-recovery-chart.ts',
  'packages/server-finance/src/adapters/manufacturing-accounting-sources.ts',
  'packages/server-finance/src/adapters/refunds.ts',
];

describe('finance active-chart base_currency read dedupe', () => {
  it('keeps one byte-identical literal and adopts no share-lock variant', async () => {
    expect(ACTIVE_CHART_BASE_CURRENCY_SQL).toBe(CHART_SQL);
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/chart-base-currency-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split(CHART_SQL).length - 1).toBe(1);
    expect(shared).not.toContain('FOR SHARE');
    for (const rel of CHART_CALL_SITES) {
      const text = await readFile(new URL(`../../../${rel}`, import.meta.url), 'utf8');
      expect(text.split(CHART_SQL).length - 1).toBe(0);
      expect(text).toContain('ACTIVE_CHART_BASE_CURRENCY_SQL');
      expect(text).not.toContain('FOR SHARE');
    }
  });

  it('drives the leaf reader through the shared SQL with unchanged binds', async () => {
    const prepares: string[] = [];
    const binds: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        prepares.push(sql);
        return { bind(...values: unknown[]) { binds.push(values); return { first: async () => ({ base_currency: 'THB' }) }; } };
      },
    };
    const { payrollRecoveryChart } = await import('../../../packages/server-finance/src/adapters/payroll-recovery-chart');
    await expect(payrollRecoveryChart(db as never).readRecognizePayrollRecoveryChart('ch-1', 'co-1'))
      .resolves.toEqual({ base_currency: 'THB' });
    expect(prepares).toEqual([CHART_SQL]);
    expect(binds).toEqual([['ch-1', 'co-1']]);
  });
});

const POSTED_CHART_SQL = "SELECT 1 FROM finance_journals WHERE chart_id=? AND status IN ('posted','reversed')";

describe('finance books posted-chart existence read dedupe', () => {
  it('keeps one byte-identical literal and two constant-backed reads', async () => {
    const text = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-books.ts', import.meta.url), 'utf8');
    expect(text.split(`"${POSTED_CHART_SQL}"`).length - 1).toBe(1);
    expect(text.split('chartPostedSql,').length - 1).toBe(2);
  });
});

const JOURNALS_CALL_SITES = [
  'packages/server-finance/src/adapters/final-root-owner-projection-sources.ts',
  'packages/server-finance/src/adapters/platform-owner-projection-sources.ts',
  'packages/server-finance/src/adapters/inventory-executive-owner-projections.ts',
  'packages/server-finance/src/adapters/hospitality-owner-projection-sources.ts',
  'packages/server-finance/src/adapters/commercial-owner-projection-sources.ts',
];

describe('finance finance_journals projection dedupe', () => {
  it('keeps one byte-identical literal and backs all five owner projections with it', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-journals-projection-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('FROM finance_journals').length - 1).toBe(1);
    for (const rel of JOURNALS_CALL_SITES) {
      const text = await readFile(new URL(`../../../${rel}`, import.meta.url), 'utf8');
      expect(text.split('FROM finance_journals').length - 1).toBe(0);
      expect(text).toContain('FINANCE_JOURNALS_PROJECTION_SQL');
    }
    expect(finalRootOwnerProjectionSources.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
    expect(platformOwnerProjectionSources.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
    expect(inventoryExecutiveOwnerProjections.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
    expect(hospitalityOwnerProjectionSources.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
    expect(commercialOwnerProjectionSources.finance_journals).toBe(FINANCE_JOURNALS_PROJECTION_SQL);
  });
});

const CHARTS_CALL_SITES = [
  'packages/server-finance/src/adapters/platform-owner-projection-sources.ts',
  'packages/server-finance/src/adapters/inventory-executive-owner-projections.ts',
  'packages/server-finance/src/adapters/commercial-owner-projection-sources.ts',
];

describe('finance finance_charts projection dedupe', () => {
  it('keeps one byte-identical literal and backs all three owner projections with it', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-charts-projection-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('FROM finance_charts').length - 1).toBe(1);
    for (const rel of CHARTS_CALL_SITES) {
      const text = await readFile(new URL(`../../../${rel}`, import.meta.url), 'utf8');
      expect(text.split('FROM finance_charts').length - 1).toBe(0);
      expect(text).toContain('FINANCE_CHARTS_PROJECTION_SQL');
    }
    expect(platformOwnerProjectionSources.finance_charts).toBe(FINANCE_CHARTS_PROJECTION_SQL);
    expect(inventoryExecutiveOwnerProjections.finance_charts).toBe(FINANCE_CHARTS_PROJECTION_SQL);
    expect(commercialOwnerProjectionSources.finance_charts).toBe(FINANCE_CHARTS_PROJECTION_SQL);
  });
});

const POSTING_PERIODS_SQL = "SELECT p.id,p.name,p.start_date,p.end_date,p.state,p.close_version version,to_jsonb(p)::text source FROM finance_periods p WHERE p.company_id=? ORDER BY p.id";
const POSTING_FX_SQL = "SELECT r.id,r.currency,r.effective_date,r.currency_per_usd::text,r.confirmed_at,to_jsonb(r)::text source FROM company_fx_rates r WHERE r.company_id=? ORDER BY r.id";
const POSTING_MONEY_SQL = "SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=? UNION ALL SELECT 'scale:'||s.currency kind,to_jsonb(s)::text source FROM company_currency_scales s WHERE s.company_id=? ORDER BY kind";
const POSTING_RECEIPT_SQL = "SELECT record_id FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";

const POSTING_CALL_SITES: [string, string[]][] = [
  ['assets-posting-prerequisites.ts', ['POSTING_FINANCE_PERIODS_SQL', 'POSTING_COMPANY_FX_RATES_SQL', 'POSTING_COMPANY_MONEY_IMAGE_SQL', 'POSTING_COMMAND_RECEIPT_RECORD_SQL']],
  ['expense-posting-prerequisites.ts', ['POSTING_FINANCE_PERIODS_SQL', 'POSTING_COMPANY_FX_RATES_SQL', 'POSTING_COMPANY_MONEY_IMAGE_SQL']],
  ['manufacturing-posting-prerequisites.ts', ['POSTING_FINANCE_PERIODS_SQL', 'POSTING_COMPANY_FX_RATES_SQL', 'POSTING_COMPANY_MONEY_IMAGE_SQL', 'POSTING_COMMAND_RECEIPT_RECORD_SQL']],
  ['payroll-posting-prerequisites.ts', ['POSTING_FINANCE_PERIODS_SQL', 'POSTING_COMPANY_FX_RATES_SQL', 'POSTING_COMPANY_MONEY_IMAGE_SQL']],
  ['pos-startup-prerequisites.ts', ['POSTING_FINANCE_PERIODS_SQL', 'POSTING_COMPANY_FX_RATES_SQL', 'POSTING_COMPANY_MONEY_IMAGE_SQL', 'POSTING_COMMAND_RECEIPT_RECORD_SQL']],
  ['sales-posting-setup.ts', ['POSTING_FINANCE_PERIODS_SQL', 'POSTING_COMPANY_FX_RATES_SQL', 'POSTING_COMPANY_MONEY_IMAGE_SQL', 'POSTING_COMMAND_RECEIPT_RECORD_SQL']],
  ['supply-posting-prerequisites.ts', ['POSTING_FINANCE_PERIODS_SQL', 'POSTING_COMPANY_FX_RATES_SQL', 'POSTING_COMPANY_MONEY_IMAGE_SQL', 'POSTING_COMMAND_RECEIPT_RECORD_SQL']],
];

describe('finance posting-prerequisites SQL dedupe', () => {
  it('keeps one byte-identical literal per family with unchanged text', async () => {
    expect(POSTING_FINANCE_PERIODS_SQL).toBe(POSTING_PERIODS_SQL);
    expect(POSTING_COMPANY_FX_RATES_SQL).toBe(POSTING_FX_SQL);
    expect(POSTING_COMPANY_MONEY_IMAGE_SQL).toBe(POSTING_MONEY_SQL);
    expect(POSTING_COMMAND_RECEIPT_RECORD_SQL).toBe(POSTING_RECEIPT_SQL);
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/posting-prerequisites-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split(POSTING_PERIODS_SQL).length - 1).toBe(1);
    expect(shared.split(POSTING_FX_SQL).length - 1).toBe(1);
    expect(shared.split(POSTING_MONEY_SQL).length - 1).toBe(1);
    expect(shared.split(POSTING_RECEIPT_SQL).length - 1).toBe(1);
    expect(shared.split('SELECT ').length - 1).toBe(5);
  });

  it('backs every call site with the shared constants and no inline literal', async () => {
    for (const [file, names] of POSTING_CALL_SITES) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      for (const lit of [POSTING_PERIODS_SQL, POSTING_FX_SQL, POSTING_MONEY_SQL, POSTING_RECEIPT_SQL]) {
        expect(text.split(lit).length - 1).toBe(0);
      }
      expect(text).toContain('from "./posting-prerequisites-dedupe-sql"');
      for (const name of names) expect(text).toContain(name);
    }
  });
});

const ADVISORY_LOCK_SQL = 'SELECT pg_advisory_xact_lock(hashtextextended(?,0))';
const ADVISORY_LOCK_CALL_SITES = [
  'finance-books.ts', 'finance-planning.ts', 'finance-tax-identities.ts',
  'finance-thai-documents.ts', 'finance-withholding-remittance.ts', 'finance-withholding.ts',
];

describe('finance advisory-lock SQL dedupe', () => {
  it('keeps one byte-identical literal and backs every lock site', async () => {
    expect(FINANCE_ADVISORY_XACT_LOCK_SQL).toBe(ADVISORY_LOCK_SQL);
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/advisory-lock-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split(ADVISORY_LOCK_SQL).length - 1).toBe(1);
    for (const file of ADVISORY_LOCK_CALL_SITES) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text.split(ADVISORY_LOCK_SQL).length - 1).toBe(0);
      expect(text).toContain('from "./advisory-lock-dedupe-sql"');
      expect(text).toContain('FINANCE_ADVISORY_XACT_LOCK_SQL');
    }
  });
});

const TAIL_IMAGE_SQL = "SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?";
const TAIL_INVOICES_SQL = "SELECT \"id\",\"company_id\",\"party_id\",\"kind\",\"invoice_number\",\"invoice_date\",\"due_date\",\"currency\",\"subtotal_minor\",\"tax_minor\",\"total_minor\",\"outstanding_minor\",\"tax_code\",\"debit_account_id\",\"credit_account_id\",\"source_type\",\"source_id\",\"status\",\"journal_id\",\"version\",\"created_by\",\"approved_by\",\"posted_at\",\"created_at\",\"updated_at\",\"tax_account_id\",\"fx_rate_micros\",\"base_total_minor\",\"base_outstanding_minor\",\"line_mode\",\"line_revision\" FROM finance_invoices";
const TAIL_PARTIES_SQL = "SELECT \"id\",\"company_id\",\"kind\",\"code\",\"name\",\"currency\",\"tax_registration\",\"payment_terms_days\",\"credit_limit_minor\",\"status\",\"version\",\"created_at\",\"updated_at\",\"person_id\",\"notification_email\",\"notification_consent\",\"merged_into_id\",\"merged_at\",\"merged_by\",\"phone\",\"address_json\" FROM finance_parties";
const TAIL_ACCOUNTS_SQL = "SELECT \"id\",\"chart_id\",\"company_id\",\"code\",\"name\",\"category\",\"parent_id\",\"currency\",\"is_control\",\"status\",\"version\",\"created_at\",\"updated_at\",\"control_kind\" FROM finance_accounts";

describe('finance read-tail r14 group 3 receipt-image dedupe', () => {
  it('keeps one byte-identical literal and backs all six call sites', async () => {
    expect(FINANCE_RECEIPT_IMAGE_SQL).toBe(TAIL_IMAGE_SQL);
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tail-dedupe-sql.ts', import.meta.url), 'utf8');
    expect(shared.split(TAIL_IMAGE_SQL).length - 1).toBe(1);
    for (const file of ['expense-posting-prerequisites.ts', 'payroll-posting-prerequisites.ts']) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text.split(TAIL_IMAGE_SQL).length - 1).toBe(0);
      expect(text).toContain('from "./finance-tail-dedupe-sql"');
      expect(text).toContain('FINANCE_RECEIPT_IMAGE_SQL');
    }
  });
});

const TAIL_PROJECTION_CASES: [string, string, string][] = [
  ['commercial-owner-projection-sources.ts', 'FINANCE_ACCOUNTS_PROJECTION_SQL', TAIL_ACCOUNTS_SQL],
  ['commercial-owner-projection-sources.ts', 'FINANCE_INVOICES_PROJECTION_SQL', TAIL_INVOICES_SQL],
  ['commercial-owner-projection-sources.ts', 'FINANCE_PARTIES_PROJECTION_SQL', TAIL_PARTIES_SQL],
  ['final-root-owner-projection-sources.ts', 'FINANCE_INVOICES_PROJECTION_SQL', TAIL_INVOICES_SQL],
  ['final-root-owner-projection-sources.ts', 'FINANCE_PARTIES_PROJECTION_SQL', TAIL_PARTIES_SQL],
  ['hospitality-owner-projection-sources.ts', 'FINANCE_ACCOUNTS_PROJECTION_SQL', TAIL_ACCOUNTS_SQL],
  ['hospitality-owner-projection-sources.ts', 'FINANCE_INVOICES_PROJECTION_SQL', TAIL_INVOICES_SQL],
  ['hospitality-owner-projection-sources.ts', 'FINANCE_PARTIES_PROJECTION_SQL', TAIL_PARTIES_SQL],
  ['platform-owner-projection-sources.ts', 'FINANCE_ACCOUNTS_PROJECTION_SQL', TAIL_ACCOUNTS_SQL],
  ['platform-owner-projection-sources.ts', 'FINANCE_INVOICES_PROJECTION_SQL', TAIL_INVOICES_SQL],
  ['platform-owner-projection-sources.ts', 'FINANCE_PARTIES_PROJECTION_SQL', TAIL_PARTIES_SQL],
  ['inventory-executive-owner-projections.ts', 'FINANCE_ACCOUNTS_PROJECTION_SQL', TAIL_ACCOUNTS_SQL],
];

describe('finance read-tail r14 group 4 owner-projection dedupe', () => {
  it('keeps one byte-identical literal per projection', async () => {
    expect(FINANCE_INVOICES_PROJECTION_SQL).toBe(TAIL_INVOICES_SQL);
    expect(FINANCE_PARTIES_PROJECTION_SQL).toBe(TAIL_PARTIES_SQL);
    expect(FINANCE_ACCOUNTS_PROJECTION_SQL).toBe(TAIL_ACCOUNTS_SQL);
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tail-dedupe-sql.ts', import.meta.url), 'utf8');
    for (const lit of [TAIL_INVOICES_SQL, TAIL_PARTIES_SQL, TAIL_ACCOUNTS_SQL]) {
      expect(shared.split(lit.replace(/"/g, '\\"')).length - 1).toBe(1);
    }
    expect(shared.split('SELECT ').length - 1).toBe(4);
  });

  it('backs every owner projection with the shared constants and no inline literal', async () => {
    const sources: Record<string, Record<string, unknown>> = {
      'commercial-owner-projection-sources.ts': commercialOwnerProjectionSources as unknown as Record<string, unknown>,
      'final-root-owner-projection-sources.ts': finalRootOwnerProjectionSources as unknown as Record<string, unknown>,
      'hospitality-owner-projection-sources.ts': hospitalityOwnerProjectionSources as unknown as Record<string, unknown>,
      'platform-owner-projection-sources.ts': platformOwnerProjectionSources as unknown as Record<string, unknown>,
      'inventory-executive-owner-projections.ts': inventoryExecutiveOwnerProjections as unknown as Record<string, unknown>,
    };
    const expected = {
      FINANCE_ACCOUNTS_PROJECTION_SQL: FINANCE_ACCOUNTS_PROJECTION_SQL,
      FINANCE_INVOICES_PROJECTION_SQL: FINANCE_INVOICES_PROJECTION_SQL,
      FINANCE_PARTIES_PROJECTION_SQL: FINANCE_PARTIES_PROJECTION_SQL,
    } as Record<string, string>;
    for (const [file, name, lit] of TAIL_PROJECTION_CASES) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text.split(lit).length - 1).toBe(0);
      expect(text).toContain('from "./finance-tail-dedupe-sql"');
      expect(text).toContain(name);
      const key = name.replace('_PROJECTION_SQL', '').toLowerCase();
      expect(sources[file]![key]).toBe(expected[name]);
    }
  });
});

const R15_CURRENCY_SQL = 'SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?';
const R15_OPEN_PERIODS_SQL = "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC";
const R15_APPROVED_RULE_SQL = "SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'";
const R15_JOURNALS_JOIN_SQL = 'JOIN (SELECT "id","company_id","chart_id","period_id","journal_number","entry_date","currency","fx_rate_micros","source_type","source_id","memo","status","is_opening","version","created_by","posted_by","posted_at","reversed_by","reversed_at","created_at","updated_at" FROM finance_journals) j';
const R15_RECEIPT_EXISTS_SQL = "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?";
const R15_RECEIPT_IMAGE_SQL = "SELECT r.record_id,to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=? AND r.operation=? AND r.actor_id=? AND r.authority_json::jsonb->>'companyId'=?";

const R15_GROUPS: [string, string, string, string[]][] = [
  ['CHART_BASE_CURRENCY_BY_ID_SQL', CHART_BASE_CURRENCY_BY_ID_SQL, R15_CURRENCY_SQL, ['pos-core-facts.ts', 'commerce-facts.ts', 'finance.ts', 'finance-collections.ts']],
  ['OPEN_PERIOD_LIST_SQL', OPEN_PERIOD_LIST_SQL, R15_OPEN_PERIODS_SQL, ['payroll-owner-facts.ts', 'finance-books.ts', 'finance-business-contracts.ts', 'finance-invoice-read.ts']],
  ['APPROVED_SECONDARY_CHART_RULE_SQL', APPROVED_SECONDARY_CHART_RULE_SQL, R15_APPROVED_RULE_SQL, ['finance-assets.ts', 'finance-business-contracts.ts', 'refund-bank-evidence.ts', 'finance-instruments.ts']],
  ['FINANCE_JOURNALS_JOIN_SQL', FINANCE_JOURNALS_JOIN_SQL, R15_JOURNALS_JOIN_SQL, ['people-payroll-final-owner-projections.ts', 'workflow-final-owner-projections.ts']],
  ['COMMAND_RECEIPT_EXISTS_SQL', COMMAND_RECEIPT_EXISTS_SQL, R15_RECEIPT_EXISTS_SQL, ['document-purchases.ts', 'finance-due-drafts.ts', 'purchase-boundary.ts']],
  ['COMMAND_RECEIPT_RECORD_IMAGE_SQL', COMMAND_RECEIPT_RECORD_IMAGE_SQL, R15_RECEIPT_IMAGE_SQL, ['expense-posting-prerequisites.ts', 'payroll-posting-prerequisites.ts']],
];

describe('finance read-tail r15 byte-identical SQL dedupe', () => {
  it('keeps one byte-identical literal per family with no inline duplicates', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r15-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(6);
    for (const [name, actual, literal, files] of R15_GROUPS) {
      expect(actual).toBe(literal);
      const sourceLiteral = literal.includes('"') ? literal.replace(/"/g, '\\"') : literal;
      expect(shared.split(sourceLiteral).length - 1).toBe(1);
      for (const file of files) {
        const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
        expect(text.split(literal).length - 1).toBe(0);
        expect(text).toContain(name);
      }
    }
  });

  it('backs both finance_journals JOIN projections with the shared text', () => {
    const p = peoplePayrollFinalOwnerProjections();
    const w = workflowFinalOwnerProjections();
    expect(p.executive_main_evidence_sources_finance_journals_j_join).toBe(FINANCE_JOURNALS_JOIN_SQL);
    expect(p.executive_metric_projections_finance_journals_j_join).toBe(FINANCE_JOURNALS_JOIN_SQL);
    expect(w.flow_routing_finance_journals_j_join).toBe(FINANCE_JOURNALS_JOIN_SQL);
  });
});

const R16_GUARD_LITERAL = "EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)";
const R16_IDS_LITERAL = "SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?";
const R16_GROUPS: [string, string, string, string[]][] = [
  ['OPEN_PERIOD_GUARD_SQL', OPEN_PERIOD_GUARD_SQL, R16_GUARD_LITERAL, ['journal-planning.ts', 'commerce-facts.ts', 'finance-owned-reversal.ts', 'inventory-transit-sources.ts', 'finance-books.ts', 'finance.ts']],
  ['OPEN_PERIOD_IDS_SQL', OPEN_PERIOD_IDS_SQL, R16_IDS_LITERAL, ['finance-withholding-remittance.ts', 'pms-hospitality-setup-facts.ts', 'inventory-consumption-sources.ts', 'payroll-owner-facts.ts', 'finance-accruals.ts', 'inventory-counts-sources.ts', 'inventory-landed-costs-sources.ts']],
];

describe('finance read-tail r16 byte-identical SQL dedupe', () => {
  it('keeps one byte-identical literal per family', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r16-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(2);
    for (const [name, actual, literal] of R16_GROUPS) {
      expect(actual).toBe(literal);
      expect(shared.split(literal).length - 1).toBe(1);
      expect(typeof name).toBe('string');
    }
  });

  it('backs all 14 call sites with the shared constants and no inline literal', async () => {
    for (const [name, , literal, files] of R16_GROUPS) {
      for (const file of files) {
        const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
        expect(text.split(literal).length - 1).toBe(0);
        expect(text).toContain("from './finance-read-tail-r16-sql'");
        expect(text).toContain(name);
      }
    }
  });

  it('leaves the FOR SHARE and ORDER BY LIMIT variants inline', async () => {
    const share = await readFile(new URL('../../../packages/server-finance/src/adapters/pms-command-facts.ts', import.meta.url), 'utf8');
    expect(share.split(`${R16_IDS_LITERAL} FOR SHARE`).length - 1).toBe(1);
    const limit = await readFile(new URL('../../../packages/server-finance/src/adapters/manufacturing-accounting-sources.ts', import.meta.url), 'utf8');
    expect(limit.split(`${R16_IDS_LITERAL} ORDER BY start_date DESC,id LIMIT 1`).length - 1).toBe(1);
  });
});

const R17_SUPPLIER_LITERAL = "NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown'))";
const R17_BANK_LITERAL = "SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?";
const R17_DIMENSION_LITERAL = "SELECT id,code,name,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code";
const R17_GROUPS: [string, string, string, string[]][] = [
  ['SUPPLIER_RESERVATION_GUARD_SQL', SUPPLIER_RESERVATION_GUARD_SQL, R17_SUPPLIER_LITERAL, ['commerce-facts.ts', 'finance-withholding.ts', 'finance.ts']],
  ['BANK_TRANSACTION_BY_ID_SQL', BANK_TRANSACTION_BY_ID_SQL, R17_BANK_LITERAL, ['finance-bank-automation.ts', 'finance.ts', 'refunds.ts']],
  ['DIMENSION_DEFINITIONS_SQL', DIMENSION_DEFINITIONS_SQL, R17_DIMENSION_LITERAL, ['finance-assets.ts', 'ledger.ts', 'record-presentation.ts']],
];

describe('finance read-tail r17 byte-identical SQL dedupe', () => {
  it('keeps one byte-identical literal per family', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r17-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(3);
    for (const [name, actual, literal] of R17_GROUPS) {
      expect(actual).toBe(literal);
      expect(shared.split(literal).length - 1).toBe(1);
      expect(typeof name).toBe('string');
    }
  });

  it('backs all 9 call sites with the shared constants and no inline literal', async () => {
    for (const [name, , literal, files] of R17_GROUPS) {
      for (const file of files) {
        const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
        expect(text.split(literal).length - 1).toBe(0);
        expect(text).toContain("from './finance-read-tail-r17-sql'");
        expect(text).toContain(name);
      }
    }
  });

  it('leaves the FOR UPDATE OF t variants inline', async () => {
    const pos = await readFile(new URL('../../../packages/server-finance/src/adapters/pos-core-facts.ts', import.meta.url), 'utf8');
    expect(pos.split(`${R17_BANK_LITERAL} FOR UPDATE OF t`).length - 1).toBe(2);
  });
});

const R18_MONEY_SETTINGS_LITERAL = "SELECT jsonb_build_object('chart',(SELECT to_jsonb(c) FROM finance_charts c WHERE c.id=? AND c.company_id=?),'settings',(SELECT to_jsonb(s) FROM company_money_settings s WHERE s.company_id=?),'scale',(SELECT to_jsonb(s) FROM company_currency_scales s WHERE s.company_id=? AND s.currency=?),'rates',COALESCE((SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM company_fx_rates r WHERE r.company_id=? AND r.currency=?),'[]'::jsonb))::text image";
const R18_SNAPSHOT_LITERAL = "SELECT s.company_id,s.source_type,s.source_id,s.amount_kind,s.transaction_date::text,s.currency,s.currency_scale,s.amount_units::text,s.usd_units::text,s.fx_rate_id,s.used_latest_fallback,s.captured_at,to_jsonb(s)::text source FROM money_transaction_snapshots s WHERE s.company_id=? AND s.source_type='commerce_product_revision' AND s.source_id=? ORDER BY s.amount_kind";
const R18_PAYMENT_LITERAL = 'SELECT "id","company_id","party_id","kind","payment_number","payment_date","currency","amount_minor","outstanding_minor","bank_account","status","journal_id","version","created_by","approved_by","released_by","created_at","updated_at","payment_slip_id","fx_rate_micros","base_amount_minor","fx_journal_id","withholding_assessment_id","withheld_minor" FROM finance_payments';
const R18_REFUND_OBLIGATION_LITERAL = 'SELECT "id","company_id","branch_id","department_id","return_id","invoice_id","party_id","direction","currency","chart_id","balance_account_id","amount_minor","outstanding_minor","status","beneficiary_name","beneficiary_account","approval_reason","created_by","approved_by","approved_at","version","created_at","updated_at","base_amount_minor","base_outstanding_minor","source_credit_journal_id","billing_correction_id" FROM finance_refund_obligations';
const R18_ACCOUNT_CHART_LITERAL = "SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?";
const R18_ALLOCATION_LITERAL = 'SELECT "id","company_id","payment_id","invoice_id","amount_minor","created_by","created_at","base_invoice_minor","base_payment_minor","realized_fx_minor" FROM finance_allocations';
const R18_PERIOD_DATES_LITERAL = "SELECT start_date,end_date FROM finance_periods WHERE id=? AND company_id=?";
const R18_COLLECTIONS_TOOL_LITERAL = "EXISTS(SELECT 1 FROM finance_company_tools WHERE company_id=? AND tool='collections' AND enabled=1)";
const R18_BANK_STATEMENT_VERSION_LITERAL = "EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.external_id=?)";
const R18_GROUPS: [string, string, string, string[]][] = [
  ['MONEY_SETTINGS_IMAGE_SQL', MONEY_SETTINGS_IMAGE_SQL, R18_MONEY_SETTINGS_LITERAL, ['manufacturing-initial-physical-stock-master-sources.ts', 'pos-startup-master-sources.ts']],
  ['MONEY_TXN_SNAPSHOT_SOURCE_SQL', MONEY_TXN_SNAPSHOT_SOURCE_SQL, R18_SNAPSHOT_LITERAL, ['manufacturing-initial-physical-stock-master-sources.ts', 'pos-startup-master-sources.ts', 'sales-service-product-sources.ts']],
  ['PAYMENT_PROJECTION_SQL', PAYMENT_PROJECTION_SQL, R18_PAYMENT_LITERAL, ['final-root-owner-projection-sources.ts', 'hospitality-owner-projection-sources.ts']],
  ['REFUND_OBLIGATION_PROJECTION_SQL', REFUND_OBLIGATION_PROJECTION_SQL, R18_REFUND_OBLIGATION_LITERAL, ['final-root-owner-projection-sources.ts', 'commercial-owner-projection-sources.ts']],
  ['ACCOUNT_CHART_ID_SQL', ACCOUNT_CHART_ID_SQL, R18_ACCOUNT_CHART_LITERAL, ['record-presentation.ts', 'assets-financial-workflow.ts']],
  ['ALLOCATION_PROJECTION_SQL', ALLOCATION_PROJECTION_SQL, R18_ALLOCATION_LITERAL, ['hospitality-owner-projection-sources.ts', 'commercial-owner-projection-sources.ts']],
  ['PERIOD_DATES_SQL', PERIOD_DATES_SQL, R18_PERIOD_DATES_LITERAL, ['finance-invoice-read.ts', 'records-record-details-owned-facts.ts']],
  ['COLLECTIONS_TOOL_GUARD_SQL', COLLECTIONS_TOOL_GUARD_SQL, R18_COLLECTIONS_TOOL_LITERAL, ['finance-collections.ts']],
  ['BANK_STATEMENT_VERSION_GUARD_SQL', BANK_STATEMENT_VERSION_GUARD_SQL, R18_BANK_STATEMENT_VERSION_LITERAL, ['payroll-other-remittance.ts', 'payroll-owner-facts.ts']],
];

describe('finance read-tail r18 byte-identical SQL dedupe', () => {
  it('keeps one byte-identical literal per family with no inline duplicates', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r18-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(13);
    for (const [name, actual, literal] of R18_GROUPS) {
      expect(actual).toBe(literal);
      expect(actual).not.toContain('FOR UPDATE');
      expect(actual).not.toContain('FOR SHARE');
      expect(shared.split(literal).length - 1).toBe(1);
      expect(typeof name).toBe('string');
    }
  });

  it('backs every call site with the shared constants and unchanged binds', async () => {
    const bindFiles = new Set([
      'manufacturing-initial-physical-stock-master-sources.ts', 'pos-startup-master-sources.ts',
      'sales-service-product-sources.ts', 'record-presentation.ts', 'assets-financial-workflow.ts',
      'finance-invoice-read.ts', 'records-record-details-owned-facts.ts', 'finance-collections.ts',
      'payroll-other-remittance.ts', 'payroll-owner-facts.ts',
    ]);
    for (const [name, , literal, files] of R18_GROUPS) {
      for (const file of files) {
        const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
        expect(text.split(literal).length - 1).toBe(0);
        expect(text).toContain(name);
        if (bindFiles.has(file)) expect(text).toContain('.bind(');
      }
    }
  });

  it('leaves every lock-taking variant inline outside the shared text', async () => {
    const pos = await readFile(new URL('../../../packages/server-finance/src/adapters/pos-core-facts.ts', import.meta.url), 'utf8');
    expect(pos.split(`${R17_BANK_LITERAL} FOR UPDATE OF t`).length - 1).toBe(2);
    const pms = await readFile(new URL('../../../packages/server-finance/src/adapters/pms-command-facts.ts', import.meta.url), 'utf8');
    expect(pms.split(`${R16_IDS_LITERAL} FOR SHARE`).length - 1).toBe(1);
  });
});

const R19_ROOT = 'SELECT 1 FROM finance_documents';
const R19_SRC = 'SELECT 1 FROM finance_owned';
const R19_OWNER_JOIN = (alias: string) =>
  `EXISTS(SELECT 1 FROM (${R19_ROOT}) root JOIN (${R19_SRC}) ${alias} ON ${alias}.id=root.id AND ${alias}.company_id=root.company_id)`;
const R19_OWNER_FILES = [
  'document-purchases.ts', 'pos-initial-stock-journal-sources.ts',
  'manufacturing-initial-stock-journal-sources.ts', 'service-initial-stock-journal-sources.ts',
];
const R19_CLOCK_FILES = [
  'group-purchase-sources.ts', 'sales-posting-setup.ts', 'document-purchases.ts',
  'finance-expansion-work.ts', 'legal-setup.ts', 'ledger.ts', 'expense-posting-prerequisites.ts',
  'finance-due-drafts.ts', 'refunds.ts', 'assets-financial-entry.ts',
  'payroll-posting-prerequisites.ts', 'sales-customer-receipts.ts', 'calendar-purchases.ts',
];

describe('finance read-tail r19 byte-identical SQL dedupe', () => {
  it('renders the root-to-owner existence join byte-identically for every alias', () => {
    for (const alias of ['owner_source', 'marker', 'source']) {
      expect(rootOwnerExistsSql(R19_ROOT, R19_SRC, alias)).toBe(R19_OWNER_JOIN(alias));
      expect(rootOwnerExistsSql(R19_ROOT, R19_SRC, alias)).not.toMatch(/FOR UPDATE|FOR SHARE/);
    }
  });

  it('renders all three materialized clock CTE variants byte-identically', () => {
    const clock = 'package_clock_sql';
    expect(materializedClockCteSql('clean_clock', clock)).toBe(`WITH clean_clock AS MATERIALIZED(SELECT ${clock} now_ms)`);
    expect(materializedClockCteSql('spaced_clock', clock, true)).toBe(`WITH spaced_clock AS MATERIALIZED (SELECT ${clock} now_ms)`);
    expect(materializedClockCteSql('as_clock', clock, true, true)).toBe(`WITH as_clock AS MATERIALIZED (SELECT ${clock} AS now_ms)`);
    expect(materializedClockCteSql('as_clock', clock, true, true)).not.toMatch(/FOR UPDATE|FOR SHARE/);
  });

  it('keeps exactly two SELECT-bearing templates in the shared helper', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r19-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(2);
  });

  it('backs all 22 owner-join sites with the helper and no inline join', async () => {
    let calls = 0;
    for (const file of R19_OWNER_FILES) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text).not.toMatch(/EXISTS\(SELECT 1 FROM \(\$\{[^}]*\}\) root JOIN \(\$\{[^}]*\}\) (owner_source|marker|source) ON \1\.id=root\.id AND \1\.company_id=root\.company_id\)/);
      expect(text).toContain('rootOwnerExistsSql');
      calls += (text.match(/rootOwnerExistsSql\(/g) ?? []).length;
    }
    expect(calls).toBe(22);
  });

  it('backs all 15 WITH-prefixed clock sites with the helper and no inline clock prefix', async () => {
    let calls = 0;
    for (const file of R19_CLOCK_FILES) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text).not.toMatch(/WITH \w+ AS MATERIALIZED\(SELECT \$\{packageClockSql\}/);
      expect(text).toContain('materializedClockCteSql');
      calls += (text.match(/materializedClockCteSql\(/g) ?? []).length;
    }
    expect(calls).toBe(15);
  });

  it('leaves the comma-joined clock CTE and lock variants inline', async () => {
    const doc = await readFile(new URL('../../../packages/server-finance/src/adapters/document-purchases.ts', import.meta.url), 'utf8');
    expect(doc.split('finance_entry_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)').length - 1).toBe(1);
    expect(doc.split('root JOIN finance_assets a ON').length - 1).toBe(1);
    expect(doc.split('root JOIN finance_asset_disposals a ON').length - 1).toBe(1);
    const pos = await readFile(new URL('../../../packages/server-finance/src/adapters/pos-core-facts.ts', import.meta.url), 'utf8');
    expect(pos.split(`${R17_BANK_LITERAL} FOR UPDATE OF t`).length - 1).toBe(2);
  });
});

const R20_GUARD_TEMPLATE = (table: string, status: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND status='${status}' AND version=?)`;
const R20_AGG_TEMPLATE = (a: string, table: string, order: string) =>
  `(SELECT jsonb_agg(to_jsonb(${a}) ORDER BY ${a}.${order}) FROM ${table} ${a} WHERE ${a}.company_id=?)`;
const R20_GUARDS: [string, string, string][] = [
  ['refunds.ts', 'finance_refund_obligations', 'pending'],
  ['refunds.ts', 'finance_refund_instructions', 'prepared'],
  ['finance.ts', 'finance_invoices', 'draft'],
  ['finance-withholding.ts', 'finance_withholding_assessments', 'consumed'],
  ['finance-withholding.ts', 'finance_withholding_assessments', 'draft'],
  ['finance-withholding.ts', 'finance_payments', 'draft'],
  ['finance-withholding-remittance.ts', 'finance_withholding_remittances', 'confirmed'],
  ['finance-withholding-remittance.ts', 'finance_withholding_packages', 'draft'],
  ['finance-withholding-remittance.ts', 'finance_withholding_packages', 'approved'],
  ['finance-thai-documents.ts', 'finance_thai_document_changes', 'draft'],
  ['finance-tax-register.ts', 'finance_vat_corrections', 'prepared'],
  ['finance-tax-register.ts', 'finance_statutory_packages', 'prepared'],
  ['finance-tax-register.ts', 'finance_statutory_packages', 'exported'],
  ['finance-tax-register.ts', 'finance_statutory_packages', 'approved_for_export'],
  ['finance-tax-identities.ts', 'finance_tax_identities', 'draft'],
  ['finance-reporting.ts', 'finance_year_closings', 'draft'],
  ['finance-reporting.ts', 'finance_statement_packages', 'draft'],
  ['finance-owned-reversal.ts', 'finance_journals', 'posted'],
  ['finance-invoice-lines.ts', 'finance_accounts', 'active'],
  ['finance-business-contracts.ts', 'finance_recognition_runs', 'posted'],
  ['finance-business-contracts.ts', 'finance_recognition_runs', 'draft'],
  ['finance-business-contracts.ts', 'finance_business_contracts', 'active'],
  ['finance-book-adjustments.ts', 'finance_book_adjustments', 'posted'],
  ['finance-book-adjustments.ts', 'finance_book_adjustments', 'draft'],
  ['finance-billing-corrections.ts', 'finance_billing_corrections', 'draft'],
  ['finance-assets.ts', 'finance_asset_disposals', 'draft'],
  ['finance-asset-policies.ts', 'finance_asset_policy_runs', 'draft'],
  ['finance-accruals.ts', 'finance_accrual_schedules', 'active'],
];
const R20_AGGS: [string, string, string, string][] = [
  ['sales-service-product-sources.ts', 'c', 'finance_charts', 'id'],
  ['sales-service-product-sources.ts', 'a', 'finance_accounts', 'id'],
  ['sales-service-product-sources.ts', 'd', 'finance_sales_defaults', 'id'],
  ['sales-service-product-sources.ts', 's', 'company_currency_scales', 'currency'],
  ['sales-service-product-sources.ts', 'r', 'company_fx_rates', 'id'],
  ['assets-financial-sources.ts', 'c', 'finance_charts', 'id'],
  ['assets-financial-sources.ts', 'a', 'finance_accounts', 'id'],
  ['assets-financial-sources.ts', 'p', 'finance_periods', 'id'],
  ['assets-financial-sources.ts', 'd', 'finance_dimension_definitions', 'id'],
  ['assets-financial-sources.ts', 'b', 'finance_book_rules', 'id'],
  ['assets-financial-sources.ts', 's', 'company_currency_scales', 'currency'],
  ['assets-financial-sources.ts', 'r', 'company_fx_rates', 'id'],
];

describe('finance read-tail r20 byte-identical SQL dedupe', () => {
  it('renders all 28 status/version guards byte-identically with no lock text', () => {
    expect(R20_GUARDS).toHaveLength(28);
    for (const [, table, status] of R20_GUARDS) {
      const rendered = idCompanyStatusVersionGuard(table, status);
      expect(rendered).toBe(R20_GUARD_TEMPLATE(table, status));
      expect(rendered).not.toMatch(/FOR UPDATE|FOR SHARE/);
    }
  });

  it('renders all 12 company-only jsonb_agg projections byte-identically with no lock text', () => {
    expect(R20_AGGS).toHaveLength(12);
    for (const [, a, table, order] of R20_AGGS) {
      const rendered = aggByCompany(a, table, order);
      expect(rendered).toBe(R20_AGG_TEMPLATE(a, table, order));
      expect(rendered).not.toMatch(/FOR UPDATE|FOR SHARE/);
    }
  });

  it('keeps exactly two SELECT-bearing templates in the shared helper', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r20-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(2);
  });

  it('backs all 28 guard sites with the helper and no inline literal', async () => {
    let calls = 0;
    for (const file of new Set(R20_GUARDS.map(([f]) => f))) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text).toContain("from './finance-read-tail-r20-sql'");
      expect(text).toContain('idCompanyStatusVersionGuard');
      for (const [f, table, status] of R20_GUARDS) {
        if (f !== file) continue;
        expect(text.split(R20_GUARD_TEMPLATE(table, status)).length - 1).toBe(0);
      }
      calls += (text.match(/idCompanyStatusVersionGuard\(/g) ?? []).length;
    }
    expect(calls).toBe(28);
  });

  it('backs the 12 company-only plus 3 variant jsonb_agg sites with the helper and keeps COALESCE outside', async () => {
    let calls = 0;
    for (const file of new Set(R20_AGGS.map(([f]) => f))) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text).toContain("from './finance-read-tail-r20-sql'");
      expect(text).toContain('aggByCompany');
      for (const [f, a, table, order] of R20_AGGS) {
        if (f !== file) continue;
        expect(text.split(R20_AGG_TEMPLATE(a, table, order)).length - 1).toBe(0);
      }
      calls += (text.match(/aggByCompany\(/g) ?? []).length;
    }
    expect(calls).toBe(15);
    const sales = await readFile(new URL('../../../packages/server-finance/src/adapters/sales-service-product-sources.ts', import.meta.url), 'utf8');
    expect(sales.split(`COALESCE(\${aggByCompany('c','finance_charts','id')},'[]'::jsonb)`).length - 1).toBe(1);
    const assets = await readFile(new URL('../../../packages/server-finance/src/adapters/assets-financial-sources.ts', import.meta.url), 'utf8');
    expect(assets.split(`COALESCE(\${aggByCompany('c','finance_charts','id')}`).length - 1).toBe(0);
  });

  it('leaves r18 predicate and lock variants inline; extra predicates now shared', async () => {
    const tax = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(tax.split("aggByCompany('t','finance_current_tax_entries','id',' AND t.chart_id=? AND t.claim_month=?')").length - 1).toBe(1);
    expect(tax.split('t.company_id=? AND t.chart_id=? AND t.claim_month=?').length - 1).toBe(0);
    const attach = await readFile(new URL('../../../packages/server-finance/src/adapters/commerce-attachment-sources.ts', import.meta.url), 'utf8');
    expect(attach.split("aggByCompany('p','finance_parties','id',' AND p.id IN(SELECT jsonb_array_elements_text(?::jsonb))')").length - 1).toBe(1);
    const r18 = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r18-sql.ts', import.meta.url), 'utf8');
    expect(r18.split('r.currency=?)').length - 1).toBeGreaterThan(0);
    const wh = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-withholding-remittance.ts', import.meta.url), 'utf8');
    expect(wh.split('SELECT id FROM finance_withholding_packages WHERE id=? FOR UPDATE').length - 1).toBe(1);
  });
});

const R21_VARIANTS: [string, string, string, string[], string][] = [
  [
    'finance-tax-register.ts',
    '(SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.company_id=? AND t.chart_id=? AND t.claim_month=?)',
    "aggByCompany('t','finance_current_tax_entries','id',' AND t.chart_id=? AND t.claim_month=?')",
    ["t", "finance_current_tax_entries", "id", " AND t.chart_id=? AND t.claim_month=?"],
    'registerJson',
  ],
  [
    'commerce-attachment-sources.ts',
    '(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM finance_parties p WHERE p.company_id=? AND p.id IN(SELECT jsonb_array_elements_text(?::jsonb)))',
    "aggByCompany('p','finance_parties','id',' AND p.id IN(SELECT jsonb_array_elements_text(?::jsonb))')",
    ["p", "finance_parties", "id", " AND p.id IN(SELECT jsonb_array_elements_text(?::jsonb))"],
    'parties',
  ],
  [
    'assets-financial-sources.ts',
    '(SELECT jsonb_agg(to_jsonb(v) ORDER BY v.dimension_id,v.code) FROM finance_dimension_values v WHERE v.company_id=?)',
    "aggByCompany('v','finance_dimension_values','dimension_id,code')",
    ["v", "finance_dimension_values", "dimension_id,code"],
    'dimensionValues',
  ],
  [
    'assets-financial-sources.ts',
    '(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.rule_id,m.primary_account_id) FROM finance_book_account_mappings m JOIN finance_book_rules b ON b.id=m.rule_id WHERE b.company_id=?)',
    "aggByCompany('m','finance_book_account_mappings','rule_id,primary_account_id','','finance_book_account_mappings m JOIN finance_book_rules b ON b.id=m.rule_id','b')",
    ["m", "finance_book_account_mappings", "rule_id,primary_account_id", "", "finance_book_account_mappings m JOIN finance_book_rules b ON b.id=m.rule_id", "b"],
    'bookMappings',
  ],
  [
    'assets-financial-sources.ts',
    "(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.source_id,m.amount_kind) FROM money_transaction_snapshots m WHERE m.company_id=? AND ((m.source_type='finance_asset' AND m.source_id=?) OR (m.source_type IN('finance_asset_revision','asset_depreciation') AND left(m.source_id,length(?)+1)=?||':') OR (m.source_type='asset_disposal' AND m.source_id IN(SELECT id FROM disposals))))",
    "aggByCompany('m','money_transaction_snapshots','source_type,source_id,amount_kind',` AND ((m.source_type='finance_asset' AND m.source_id=?) OR (m.source_type IN('finance_asset_revision','asset_depreciation') AND left(m.source_id,length(?)+1)=?||':') OR (m.source_type='asset_disposal' AND m.source_id IN(SELECT id FROM disposals)))`)",
    ["m", "money_transaction_snapshots", "source_type,source_id,amount_kind", " AND ((m.source_type='finance_asset' AND m.source_id=?) OR (m.source_type IN('finance_asset_revision','asset_depreciation') AND left(m.source_id,length(?)+1)=?||':') OR (m.source_type='asset_disposal' AND m.source_id IN(SELECT id FROM disposals)))"],
    'money',
  ],
];

describe('finance read-tail r21 byte-identical SQL variants', () => {
  it('renders all 5 extra-predicate / multi-column-order variants byte-identically with no lock text', () => {
    expect(R21_VARIANTS).toHaveLength(5);
    for (const [, expected, , args] of R21_VARIANTS) {
      const rendered = aggByCompany(...(args as [string, string, string, string, string, string]));
      expect(rendered).toBe(expected);
      expect(rendered).not.toMatch(/FOR UPDATE|FOR SHARE/);
    }
  });

  it('backs all 5 variant sites with the helper and no inline literal', async () => {
    for (const file of new Set(R21_VARIANTS.map(([f]) => f))) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text).toContain("from './finance-read-tail-r20-sql'");
      expect(text).toContain('aggByCompany');
      for (const [f, expected, call] of R21_VARIANTS) {
        if (f !== file) continue;
        expect(text.split(expected).length - 1).toBe(0);
        expect(text.split(call).length - 1).toBe(1);
      }
    }
  });

  it('keeps COALESCE outside the shared register projection and locks inline', async () => {
    const tax = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(tax.split("COALESCE(${aggByCompany('t','finance_current_tax_entries','id',' AND t.chart_id=? AND t.claim_month=?')},'[]'::jsonb)::text").length - 1).toBe(1);
    const wh = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-withholding-remittance.ts', import.meta.url), 'utf8');
    expect(wh.split('SELECT id FROM finance_withholding_packages WHERE id=? FOR UPDATE').length - 1).toBe(1);
  });

  it('keeps exactly two SELECT-bearing templates in the shared helper', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r20-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(2);
  });
});

const R22_EVIDENCE_TEMPLATE = (ref: string) =>
  `(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM finance_journal_lines line WHERE line.journal_id=${ref})`;
const R22_EVIDENCE_SITES: [string, string][] = [
  ['bank-statement-settlement.ts', 'journal.id'],
  ['sales-receipt-sources.ts', 'journal.id'],
  ['supplier-payment-correction-sources.ts', 'original.id'],
  ['supplier-payment-correction-sources.ts', 'correction.id'],
  ['supply-package-obligations.ts', 'journal.id'],
];
const R22_MONEY_EXTRA = ' AND (m.source_id IN(SELECT id FROM journals) OR m.source_id IN(SELECT id FROM owner_orders))';
const R22_MONEY_SQL = `(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.source_id,m.amount_kind) FROM money_transaction_snapshots m WHERE m.company_id=?${R22_MONEY_EXTRA})`;

describe('finance read-tail r22 byte-identical SQL families', () => {
  it('renders the captured journal-line evidence projection byte-identically for every ref', () => {
    for (const [, ref] of R22_EVIDENCE_SITES) {
      expect(journalLineEvidenceSql(ref)).toBe(R22_EVIDENCE_TEMPLATE(ref));
      expect(journalLineEvidenceSql(ref)).not.toMatch(/FOR UPDATE|FOR SHARE/);
    }
  });

  it('backs all five evidence sites with the helper and no inline literal', async () => {
    let calls = 0;
    for (const file of new Set(R22_EVIDENCE_SITES.map(([f]) => f))) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text).toContain("from './finance-read-tail-r22-sql'");
      expect(text).toContain('journalLineEvidenceSql');
      for (const [f, ref] of R22_EVIDENCE_SITES) {
        if (f !== file) continue;
        expect(text.split(R22_EVIDENCE_TEMPLATE(ref)).length - 1).toBe(0);
        expect(text.split("${journalLineEvidenceSql('" + ref + "')}").length - 1).toBe(1);
      }
      calls += (text.match(/journalLineEvidenceSql\(/g) ?? []).length;
    }
    expect(calls).toBe(5);
  });

  it('keeps exactly one SELECT-bearing template in the r22 helper', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r22-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(1);
  });

  it('renders the manufacturing money extra-predicate variant byte-identically with no lock text', () => {
    const rendered = aggByCompany('m', 'money_transaction_snapshots', 'source_type,source_id,amount_kind', R22_MONEY_EXTRA);
    expect(rendered).toBe(R22_MONEY_SQL);
    expect(rendered).not.toMatch(/FOR UPDATE|FOR SHARE/);
  });

  it('backs the manufacturing money site with the shared helper and no inline literal', async () => {
    const text = await readFile(new URL('../../../packages/server-finance/src/adapters/manufacturing-existing-posting-setup.ts', import.meta.url), 'utf8');
    expect(text).toContain("from './finance-read-tail-r20-sql'");
    expect(text.split(R22_MONEY_SQL).length - 1).toBe(0);
    expect(text.split("aggByCompany('m','money_transaction_snapshots','source_type,source_id,amount_kind',' AND (m.source_id IN(SELECT id FROM journals) OR m.source_id IN(SELECT id FROM owner_orders))')").length - 1).toBe(1);
  });

  it('backs the familyEvidence entries projection with the shared helper and no inline literal', async () => {
    const text = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(text.split('(SELECT jsonb_agg(to_jsonb(t) ORDER BY t.id) FROM finance_current_tax_entries t WHERE t.company_id=? AND').length - 1).toBe(0);
    expect(text.split("aggByCompany('t','finance_current_tax_entries','id',` AND").length - 1).toBe(1);
  });
});
