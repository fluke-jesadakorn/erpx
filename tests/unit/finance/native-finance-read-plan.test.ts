import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { readFile } from 'node:fs/promises';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
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
import { bookRootUnionSql, peopleRunsCteSql, peoplePostingsCteSql, journalAggregateCtesSql, netPayableFactSql, payrollUnpostedFactSql, statutoryLiabilityFactSql, statutoryLateralSql, secondaryJournalFactSql } from '../../../packages/server-finance/src/adapters/finance-read-tail-r23-sql';
import { packageObligationFactsSql, ASSET_DEPRECIATION_COUNT_SQL, ASSET_POLICY_EDITION_PENDING_SQL, ASSET_POLICY_RUN_PENDING_SQL, TAX_SOURCE_APPROVED_INVOICE_SQL, TAX_SOURCE_APPROVED_COMMERCE_RETURN_SQL, TAX_SOURCE_APPROVED_BILLING_CREDIT_SQL } from '../../../packages/server-finance/src/adapters/finance-read-tail-r24-sql';
import { ACCOUNT_SELECT_PREFIX, INVOICE_EXISTS_PREFIX, JOURNAL_SELECT_PREFIX } from '../../../packages/server-finance/src/adapters/finance-read-tail-r30-sql';
import { CONSOLIDATION_RUN_BY_ID_SQL, ISSUED_THAI_DOCUMENT_BY_ID_SQL } from '../../../packages/server-finance/src/adapters/finance-read-tail-r31-sql';
import { ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL } from '../../../packages/server-finance/src/adapters/finance-read-tail-r32-sql';
import { PERIOD_BY_ID_SQL } from '../../../packages/server-finance/src/adapters/finance-read-tail-r33-sql';
import {
  OPEN_PERIOD_COVERED_SQL,
  OPEN_PERIOD_LIST_ASC_SQL,
  BANK_TRANSACTION_STATEMENT_SQL,
  ACTIVE_CHART_NAV_SQL,
  ACTIVE_ACCOUNT_CHART_ID_SQL,
} from '../../../packages/server-finance/src/adapters/finance-read-tail-r34-sql';
import { peoplePayrollFinalOwnerProjections } from '../../../packages/server-finance/src/adapters/people-payroll-final-owner-projections';
import { workflowFinalOwnerProjections } from '../../../packages/server-finance/src/adapters/workflow-final-owner-projections';
import {
  nativeFinanceReadAll,
  nativeFinanceReadFirst,
  nativeFinanceReadScalarFirst,
  FINANCE_RD_TAX_PENDING_CORRECTION_COUNT,
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
    expect(text.split(`"${POSTED_CHART_SQL}"`).length - 1).toBe(0);
    expect(text).toContain("JOURNAL_SELECT_PREFIX + \"s WHERE chart_id=? AND status IN ('posted','reversed')\"");
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

const R23_ROOT = "SELECT ${alias}.id,${alias}.company_id UNION SELECT book.primary_journal_id,${alias}.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=${alias}.id AND book.company_id=${alias}.company_id";
const R23_RUNS = "runs AS MATERIALIZED(SELECT r.* FROM (${people.runs}) r,scope WHERE r.company_id=scope.company_id),";
const R23_POSTINGS = "postings AS MATERIALIZED(SELECT p.*,r.company_id,r.currency FROM (${people.postings}) p JOIN runs r ON r.id=p.run_id),";
const R23_JOURNALS = "journal_ids AS MATERIALIZED(SELECT id FROM owned_journals UNION SELECT book.secondary_journal_id FROM finance_book_postings book JOIN owned_journals owned ON owned.id=book.primary_journal_id),\n journals AS MATERIALIZED(SELECT j.*,COALESCE((SELECT SUM(l.debit_minor-l.credit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id),0) imbalance,\n (SELECT COUNT(*) FROM finance_journal_lines l WHERE l.journal_id=j.id) line_count,COALESCE((SELECT SUM(l.debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id),0) debit_total";
const R23_NET_PAYABLE = "UNION ALL SELECT 'net-payable',r.id,r.version,r.status,to_jsonb(r),(SELECT COALESCE(SUM(i.net_minor),0) FROM (${people.items}) i WHERE i.run_id=r.id AND i.status='posted')<>COALESCE((SELECT SUM(item.amount_minor) FROM batches b JOIN (${people.batchItems}) item ON item.batch_id=b.id WHERE b.source_type='payroll' AND b.source_id=r.id AND b.status='settled' AND item.status='settled'),0) FROM runs r WHERE r.status='posted'";
const R23_UNPOSTED = "UNION ALL SELECT 'payroll-unposted',r.id,r.version,r.status,to_jsonb(r),r.status<>'cancelled' AND (r.status<>'posted' OR NOT EXISTS(SELECT 1 FROM postings p WHERE p.run_id=r.id)) FROM runs r";
const R23_STATUTORY = "UNION ALL SELECT 'statutory-liability',p.run_id||':'||line.description,1,line.description,jsonb_build_object('posting',to_jsonb(p),'accountId',line.account_id,'amount',line.amount),line.amount<>COALESCE((SELECT SUM(remittance.amount_minor) FROM (${people.remittances}) remittance WHERE remittance.company_id=p.company_id AND remittance.run_id=p.run_id AND remittance.kind=CASE WHEN line.description='Payroll tax' THEN 'withholding_tax' ELSE 'social_security' END),0)";
const R23_LATERAL = "FROM postings p CROSS JOIN LATERAL(SELECT l.account_id,l.description,SUM(l.credit_minor-l.debit_minor) amount FROM finance_journal_lines l WHERE l.journal_id=p.journal_id AND l.description IN ('Payroll tax','Payroll social security') GROUP BY l.description,l.account_id) line";
const R23_SECONDARY = "UNION ALL SELECT 'secondary-journal',book.secondary_journal_id,book.source_version,'posted',to_jsonb(book)||jsonb_build_object('journal',to_jsonb(secondary),'rule',to_jsonb(rule)),book.company_id IS DISTINCT FROM owned.company_id OR primary_journal.id IS NULL OR secondary.id IS NULL OR secondary.status<>'posted' OR secondary.imbalance<>0 OR secondary.line_count<2 OR rule.id IS NULL OR rule.company_id IS DISTINCT FROM owned.company_id OR rule.status<>'approved' OR rule.reviewed_by IS NULL OR rule.reviewed_by=rule.prepared_by OR rule.primary_chart_id IS DISTINCT FROM primary_journal.chart_id OR rule.secondary_chart_id IS DISTINCT FROM secondary.chart_id OR secondary.currency IS DISTINCT FROM primary_journal.currency FROM owned_journals owned JOIN finance_book_postings book ON book.primary_journal_id=owned.id LEFT JOIN journals primary_journal ON primary_journal.id=owned.id LEFT JOIN journals secondary ON secondary.id=book.secondary_journal_id LEFT JOIN finance_book_rules rule ON rule.id=book.rule_id";

describe('finance read-tail r23 byte-identical SQL families', () => {
  it('renders every shared fragment byte-identically and without lock text', () => {
    const rendered = [
      bookRootUnionSql('${alias}'),peopleRunsCteSql('${people.runs}'),peoplePostingsCteSql('${people.postings}'),
      journalAggregateCtesSql(),netPayableFactSql('${people.items}','${people.batchItems}'),payrollUnpostedFactSql(),
      statutoryLiabilityFactSql('${people.remittances}'),statutoryLateralSql(),secondaryJournalFactSql(),
    ];
    for (const sql of rendered) expect(sql).not.toMatch(/FOR UPDATE|FOR SHARE/);
    expect(rendered).toEqual([
      R23_ROOT,R23_RUNS,R23_POSTINGS,R23_JOURNALS,R23_NET_PAYABLE,R23_UNPOSTED,R23_STATUTORY,R23_LATERAL,R23_SECONDARY,
    ]);
  });

  it('backs all three book-root sites with the helper and no inline literal', async () => {
    for (const file of ['pos','manufacturing','service']) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}-initial-stock-journal-sources.ts`, import.meta.url), 'utf8');
      expect(text).toContain("from './finance-read-tail-r23-sql'");
      expect(text.split("const roots=").length - 1).toBe(0);
      expect(text.split("bookRootUnionSql(alias)").length - 1).toBe(2);
    }
  });

  it('backs both People fact families with the helpers and no inline literal', async () => {
    for (const file of ['people-package-obligations.ts','people-addon-package-obligations.ts']) {
      const text = await readFile(new URL(`../../../packages/server-finance/src/adapters/${file}`, import.meta.url), 'utf8');
      expect(text).toContain("from './finance-read-tail-r23-sql'");
      for (const literal of [R23_RUNS,R23_POSTINGS,R23_NET_PAYABLE,R23_UNPOSTED,R23_STATUTORY,R23_LATERAL])
        expect(text.split(literal).length - 1).toBe(0);
      expect(text.split('UNION ALL SELECT \'secondary-journal\'').length - 1).toBe(file === 'people-addon-package-obligations.ts' ? 1 : 0);
    }
    const addon = await readFile(new URL('../../../packages/server-finance/src/adapters/people-addon-package-obligations.ts', import.meta.url), 'utf8');
    expect(addon.split("journalAggregateCtesSql()").length - 1).toBe(2);
    expect(addon.split("SELECT COUNT(*) FROM finance_journal_lines l WHERE l.journal_id=j.id) line_count,COALESCE").length - 1).toBe(0);
  });

  it('keeps the r23 helper SELECT count at the deduplicated total', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r23-sql.ts', import.meta.url), 'utf8');
    expect(shared.split('SELECT ').length - 1).toBe(19);
  });
});


const R24_POS_SQL = "WITH scope AS (SELECT ?::text company_id)\n    SELECT 'invoice' kind,i.id,i.version,i.status state,i.outstanding_minor::text amount FROM finance_invoices i,scope\n      WHERE i.company_id=scope.company_id AND i.source_type='pos' AND (i.outstanding_minor<>0 OR i.status NOT IN ('paid','voided'))\n    UNION ALL SELECT 'payment',p.id,p.version,p.status,p.outstanding_minor::text FROM finance_payments p,scope\n      WHERE p.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${pos.payments}) source WHERE source.company_id=p.company_id AND source.finance_payment_id=p.id)\n        AND (p.status NOT IN ('settled','voided') OR p.outstanding_minor<>0)\n    UNION ALL SELECT 'posting',pp.source_key,COALESCE(j.version,0),COALESCE(j.status,'missing'),COALESCE(sum(l.base_debit_minor-l.base_credit_minor),0)::text\n      FROM (${pos.postings}) pp LEFT JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=pp.company_id\n      LEFT JOIN finance_journal_lines l ON l.journal_id=j.id,scope WHERE pp.company_id=scope.company_id\n      GROUP BY pp.source_key,j.id,j.version,j.status HAVING j.id IS NULL OR j.status<>'posted' OR count(l.id)<2 OR sum(l.base_debit_minor-l.base_credit_minor)<>0\n    UNION ALL SELECT 'clearing',concat(l.account_id,':',j.currency),0,'unsettled',sum(l.debit_minor-l.credit_minor)::text\n      FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN (${pos.settings}) c ON c.company_id=j.company_id,scope\n      WHERE j.company_id=scope.company_id AND j.status='posted' AND l.account_id IN (\n        c.accounts_json::jsonb->>'stripe',c.accounts_json::jsonb->>'advance',c.accounts_json::jsonb->>'refund',c.accounts_json::jsonb->>'disputes',c.accounts_json::jsonb->>'receivable')\n      GROUP BY l.account_id,j.currency HAVING sum(l.debit_minor-l.credit_minor)<>0 ORDER BY kind,id";
const R24_PMS_SQL = "WITH scope AS (SELECT ?::text company_id)\n    SELECT 'invoice' kind,i.id,i.version,i.status state,i.outstanding_minor::text amount FROM finance_invoices i,scope\n      WHERE i.company_id=scope.company_id AND i.source_type='pms' AND (i.outstanding_minor<>0 OR i.status NOT IN ('paid','voided'))\n    UNION ALL SELECT 'payment',p.id,p.version,p.status,p.outstanding_minor::text FROM finance_payments p,scope\n      WHERE p.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${pms.payments}) source WHERE source.company_id=p.company_id AND source.finance_id=p.id)\n        AND (p.status NOT IN ('settled','voided') OR p.outstanding_minor<>0)\n    UNION ALL SELECT 'posting',concat(pp.property_id,':',pp.source_key),COALESCE(j.version,0),COALESCE(j.status,'missing'),COALESCE(sum(l.base_debit_minor-l.base_credit_minor),0)::text\n      FROM (${pms.postings}) pp LEFT JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=pp.company_id\n      LEFT JOIN finance_journal_lines l ON l.journal_id=j.id,scope WHERE pp.company_id=scope.company_id\n      GROUP BY pp.property_id,pp.source_key,j.id,j.version,j.status HAVING j.id IS NULL OR j.status<>'posted' OR count(l.id)<2 OR sum(l.base_debit_minor-l.base_credit_minor)<>0\n    UNION ALL SELECT 'clearing',concat(l.account_id,':',j.currency),0,'unsettled',sum(l.debit_minor-l.credit_minor)::text\n      FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id,scope\n      WHERE j.company_id=scope.company_id AND j.status='posted' AND EXISTS(SELECT 1 FROM (${pms.clearing}) c WHERE c.company_id=j.company_id AND c.account_id=l.account_id)\n      GROUP BY l.account_id,j.currency HAVING sum(l.debit_minor-l.credit_minor)<>0 ORDER BY kind,id";
const R24_POS_ARGS = {
  sourceType: 'pos', payments: '${pos.payments}', paymentFk: 'finance_payment_id',
  postingKey: 'pp.source_key', postings: '${pos.postings}', postingGroup: 'pp.source_key',
  clearingFrom: 'FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN (${pos.settings}) c ON c.company_id=j.company_id,scope',
  clearingWhere: "l.account_id IN (\n        c.accounts_json::jsonb->>'stripe',c.accounts_json::jsonb->>'advance',c.accounts_json::jsonb->>'refund',c.accounts_json::jsonb->>'disputes',c.accounts_json::jsonb->>'receivable')",
};
const R24_PMS_ARGS = {
  sourceType: 'pms', payments: '${pms.payments}', paymentFk: 'finance_id',
  postingKey: "concat(pp.property_id,':',pp.source_key)", postings: '${pms.postings}', postingGroup: 'pp.property_id,pp.source_key',
  clearingFrom: 'FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id,scope',
  clearingWhere: 'EXISTS(SELECT 1 FROM (${pms.clearing}) c WHERE c.company_id=j.company_id AND c.account_id=l.account_id)',
};
const R24_RENDERED: Record<string, string> = { 'pos-package-obligations.ts': R24_POS_SQL, 'pms-package-obligations.ts': R24_PMS_SQL };
const R24_ARGS: Record<string, Parameters<typeof packageObligationFactsSql>[0]> = { 'pos-package-obligations.ts': R24_POS_ARGS, 'pms-package-obligations.ts': R24_PMS_ARGS };

describe('finance read-tail r24 byte-identical SQL families', () => {
  it('renders pos and pms package-obligation skeletons byte-identically with no lock text', () => {
    for (const file of Object.keys(R24_RENDERED)) {
      const rendered = packageObligationFactsSql(R24_ARGS[file]);
      expect(rendered).toBe(R24_RENDERED[file]);
      expect(rendered).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
    }
  });

  it('keeps every shared fragment byte-identical to the pre-dedupe literal', () => {
    expect(ASSET_DEPRECIATION_COUNT_SQL).toBe("SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?");
    expect(ASSET_POLICY_EDITION_PENDING_SQL).toBe("SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status='draft'");
    expect(ASSET_POLICY_RUN_PENDING_SQL).toBe("SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND status='draft'");
    expect(TAX_SOURCE_APPROVED_INVOICE_SQL).toBe("NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')");
    expect(TAX_SOURCE_APPROVED_COMMERCE_RETURN_SQL).toBe("NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')");
    expect(TAX_SOURCE_APPROVED_BILLING_CREDIT_SQL).toBe("NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')");
  });

  it('backs every call site with the shared r24 text and no inline literal', async () => {
    for (const file of Object.keys(R24_RENDERED)) {
      const text = await readFile(new URL('../../../packages/server-finance/src/adapters/' + file, import.meta.url), 'utf8');
      expect(text).toContain('from "./finance-read-tail-r24-sql"');
      expect(text.split(R24_RENDERED[file]).length - 1).toBe(0);
      expect(text.split('packageObligationFactsSql(').length - 1).toBe(1);
    }
    const asset = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-asset-policies.ts', import.meta.url), 'utf8');
    expect(asset).toContain("from './finance-read-tail-r24-sql'");
    for (const lit of ["SELECT count(*) n FROM finance_asset_depreciations WHERE asset_id=?", "SELECT 1 FROM finance_asset_policy_editions WHERE asset_id=? AND status='draft'", "SELECT 1 FROM finance_asset_policy_runs WHERE asset_id=? AND status='draft'"]) expect(asset.split(lit).length - 1).toBe(0);
    for (const name of ['ASSET_DEPRECIATION_COUNT_SQL', 'ASSET_POLICY_EDITION_PENDING_SQL', 'ASSET_POLICY_RUN_PENDING_SQL']) expect(asset).toContain(name);
    const tax = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(tax).toContain("from './finance-read-tail-r24-sql'");
    for (const lit of ["NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=i.company_id AND t.source_type='invoice' AND t.source_id=i.id AND t.status='approved')", "NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='commerce_return' AND t.source_id=r.id AND t.status='approved')", "NOT EXISTS(SELECT 1 FROM finance_current_tax_entries t WHERE t.company_id=r.company_id AND t.source_type='billing_credit' AND t.source_id=r.id AND t.status='approved')"]) expect(tax.split(lit).length - 1).toBe(0);
    for (const name of ['TAX_SOURCE_APPROVED_INVOICE_SQL', 'TAX_SOURCE_APPROVED_COMMERCE_RETURN_SQL', 'TAX_SOURCE_APPROVED_BILLING_CREDIT_SQL']) expect(tax).toContain(name);
  });

  it('keeps the transaction locks inline and the shared helper lock-free', async () => {
    const shared = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r24-sql.ts', import.meta.url), 'utf8');
    expect(shared).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
    for (const file of ['pos-package-obligations.ts', 'pms-package-obligations.ts']) {
      const text = await readFile(new URL('../../../packages/server-finance/src/adapters/' + file, import.meta.url), 'utf8');
      expect(text.split('LOCK TABLE finance_invoices,finance_payments,finance_journals,finance_journal_lines,finance_allocations IN SHARE MODE').length - 1).toBe(1);
    }
  });

  it('retirement:ts-statements stays strictly below the tax-read-port BEFORE=2038 baseline', () => {
    const root = resolve(__dirname, '../../../packages/server-finance/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(2038);
    expect(total).toBe(1974);
  });
});

describe('finance native tax-register read-port cutover', () => {
  const TAX_KINDS = [
    'FINANCE_RD_TAX_PROFILE', 'FINANCE_RD_TAX_PACKAGE', 'FINANCE_RD_TAX_CORRECTION',
    'FINANCE_RD_TAX_ENTRY', 'FINANCE_RD_TAX_PREVIOUS_ENTRY', 'FINANCE_RD_TAX_ORIGINAL_ENTRY',
    'FINANCE_RD_TAX_PENDING_FAMILY', 'FINANCE_RD_TAX_PROFILES', 'FINANCE_RD_TAX_CHART',
    'FINANCE_RD_TAX_CURRENT_ENTRY', 'FINANCE_RD_TAX_PENDING_CORRECTION_COUNT',
    'FINANCE_RD_TAX_CLOSE_PROFILE',
  ];

  it('exposes every tax read kind with the exact registered discriminator', async () => {
    const bridge = await readFile(new URL('../../../packages/server-finance/src/adapters/native-finance-read.ts', import.meta.url), 'utf8');
    for (const kind of TAX_KINDS) {
      expect(bridge).toContain(`export const ${kind} = '${kind.replace(/^FINANCE_RD_/, 'finance_rd_').toLowerCase()}'`);
    }
    expect(bridge).toContain('export async function nativeFinanceReadAll');
    expect(bridge).toContain('export async function nativeFinanceReadFirst');
    expect(bridge).toContain('export async function nativeFinanceReadScalarFirst');
  });

  it('switches the fixed point and count readers while keeping guards and locks in TS', async () => {
    const tax = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(tax).toContain('from "./native-finance-read"');
    for (const kind of TAX_KINDS) expect(tax).toContain(kind);
    expect(tax.match(/nativeFinanceReadAll</g)?.length).toBe(1);
    expect(tax.match(/nativeFinanceReadFirst</g)?.length).toBe(10);
    expect(tax.match(/nativeFinanceReadScalarFirst</g)?.length).toBe(2);
    expect(tax.split("await db(actor),'count')").length - 1).toBe(2);
    // The other count port (`missingSources`) keeps its TS `.first('count')`.
    expect(tax.split(".first<number>('count')").length - 1).toBe(1);
    // None of the eleven switched literals remains inline in the owner.
    for (const literal of [
      "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND statutory_chart_id=?",
      "SELECT * FROM finance_statutory_packages WHERE id=? AND company_id=?",
      "SELECT * FROM finance_vat_corrections WHERE id=? AND company_id=?",
      "SELECT * FROM finance_tax_entries WHERE id=? AND company_id=?",
      "SELECT * FROM finance_current_tax_entries WHERE id=? AND company_id=? AND status='approved'",
      "SELECT * FROM finance_current_tax_entries WHERE company_id=? AND source_type='invoice' AND source_id=?",
      ".prepare(\"SELECT 1 FROM finance_vat_corrections WHERE company_id=? AND family_source_id=? AND status='prepared'\")",
      "SELECT * FROM finance_accounting_profiles WHERE company_id=? AND status='approved'",
      "SELECT 1 FROM finance_charts WHERE id=? AND company_id=?",
      "SELECT id FROM finance_current_tax_entries WHERE company_id=? AND source_type=? AND source_id=?",
      "SELECT id,statutory_chart_id,effective_date FROM finance_accounting_profiles",
    ]) expect(tax.split(literal).length - 1).toBe(0);
    // Assertion EXISTS guards and the purchased transaction boundary stay TS.
    expect((tax.match(/assertion\(write/g) ?? []).length).toBe(22);
    expect(tax).toContain('withFinancePurchasedCommand');
    expect(tax).toContain('withFinancePurchasedRead');
    expect(tax).toContain('vat-correction-volume');
    // The retained count literal is still needed ONLY by the assertion guard;
    // neither count caller prepares it in TypeScript any more.
    expect(tax.split('const pendingCorrectionsSql = "SELECT COUNT(*) FROM finance_vat_corrections').length - 1).toBe(1);
    expect(tax.split('.prepare(pendingCorrectionsSql)').length - 1).toBe(0);
    expect(tax.split('(${pendingCorrectionsSql})=0').length - 1).toBe(1);
  });
});

describe('native VAT pending-correction count scalar port', () => {
  const countSql = "SELECT COUNT(*) FROM finance_vat_corrections c JOIN finance_tax_entries t ON t.id=c.root_entry_id WHERE c.company_id=? AND c.chart_id=? AND c.status='prepared' AND t.accounting_date<=?";
  const values = ['company', 'chart', '2026-05-31'];
  let bodies: { kind: string; values: unknown[] }[] = [];

  beforeEach(() => {
    bodies = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      bodies.push(JSON.parse(String(init.body)) as { kind: string; values: unknown[] });
      return new Response(JSON.stringify({ data: { statements: [{ sql: countSql, values }] } }), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    }));
  });

  const dbWith = (rows: Record<string, unknown>[]) => {
    const statement = { bind: (..._values: unknown[]) => statement };
    return {
      prepare: () => statement,
      batch: async () => [{ results: rows, success: true, meta: { changes: 0, duration: 0 } }],
    } as never;
  };

  it('extracts the count column for two pending corrections, not the row object', async () => {
    const value = await nativeFinanceReadScalarFirst<number>(
      FINANCE_RD_TAX_PENDING_CORRECTION_COUNT, values, dbWith([{ count: '2' }]), 'count',
    );
    // The retired `.first<number>('count')` yielded a scalar; Number({count:'2'})
    // is NaN and would make every VAT package readiness check fail.
    expect(value).toBe('2');
    expect(Number(value ?? 0)).toBe(2);
    expect(bodies[0]!.kind).toBe('finance_rd_tax_pending_correction_count');
    expect(bodies[0]!.values).toEqual(values);
  });

  it('extracts a zero count without coercing the whole row', async () => {
    const value = await nativeFinanceReadScalarFirst<number>(
      FINANCE_RD_TAX_PENDING_CORRECTION_COUNT, values, dbWith([{ count: 0 }]), 'count',
    );
    expect(Number(value ?? 0)).toBe(0);
  });

  it('returns null for a zero-row result exactly like .first(column)', async () => {
    const value = await nativeFinanceReadScalarFirst<number>(
      FINANCE_RD_TAX_PENDING_CORRECTION_COUNT, values, dbWith([]), 'count',
    );
    expect(value).toBeNull();
    expect(Number(value ?? 0)).toBe(0);
  });
});

describe('finance read-tail r30 byte-identical guard-prefix families', () => {
  const PREFIXES = {
    ACCOUNT_SELECT_PREFIX: 'SELECT id FROM finance_accounts WHERE ',
    INVOICE_EXISTS_PREFIX: 'EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=?',
    JOURNAL_SELECT_PREFIX: 'SELECT 1 FROM finance_journal',
  };

  it('exposes the three exact prefixes with no lock or suffix text', () => {
    expect(ACCOUNT_SELECT_PREFIX).toBe(PREFIXES.ACCOUNT_SELECT_PREFIX);
    expect(INVOICE_EXISTS_PREFIX).toBe(PREFIXES.INVOICE_EXISTS_PREFIX);
    expect(JOURNAL_SELECT_PREFIX).toBe(PREFIXES.JOURNAL_SELECT_PREFIX);
    for (const value of Object.values(PREFIXES)) expect(value).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });

  it('leaves every row-lock suffix inline at its call site', async () => {
    const pos = await readFile(new URL('../../../packages/server-finance/src/adapters/pos-core-facts.ts', import.meta.url), 'utf8');
    expect(pos.split("AND (currency IS NULL OR currency=?) FOR SHARE").length - 1).toBe(1);
    expect(pos.split("ORDER BY code,id FOR UPDATE").length - 1).toBe(1);
  });

  it('keeps the retained tax assertion guard pendingCorrectionsSql inline', async () => {
    const tax = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(tax).toContain('const pendingCorrectionsSql = "SELECT COUNT(*) FROM finance_vat_corrections');
    expect(tax).toContain('`(${pendingCorrectionsSql})=0`');
  });

  it('backs every in-scope call site with a shared prefix and no inline family literal', async () => {
    const files = [
      'pms-command-facts.ts', 'finance-reporting.ts', 'payroll-owner-facts.ts',
      'payroll-remittance-sources.ts', 'commerce-facts.ts', 'pos-core-facts.ts',
      'refunds.ts', 'journal-planning.ts', 'finance.ts', 'finance-billing-corrections.ts',
      'finance-business-contracts.ts', 'finance-invoice-lines.ts', 'finance-thai-documents.ts',
      'hospitality-tax-documents.ts', 'invoice-payment-evidence.ts', 'service-payment-purchases.ts',
      'document-purchases.ts', 'finance-books.ts', 'manufacturing-initial-stock-journal-sources.ts',
      'pos-initial-stock-journal-sources.ts', 'service-initial-stock-journal-sources.ts',
    ];
    for (const file of files) {
      const text = await readFile(new URL('../../../packages/server-finance/src/adapters/' + file, import.meta.url), 'utf8');
      expect(text).toContain('from \'./finance-read-tail-r30-sql\'');
      expect(text.split(PREFIXES.ACCOUNT_SELECT_PREFIX).length - 1).toBe(0);
      expect(text.split(PREFIXES.INVOICE_EXISTS_PREFIX).length - 1).toBe(0);
      // no SQL literal starts with the journal prefix; suffix text after the
      // interpolation belongs to EXISTS(...) subqueries and stays inline.
      expect(text).not.toMatch(/["'`]SELECT 1 FROM finance_journal/s);
      expect(text).toMatch(/ACCOUNT_SELECT_PREFIX|INVOICE_EXISTS_PREFIX|JOURNAL_SELECT_PREFIX/);
    }
  });

  it('retirement:ts-statements stays strictly below the r30 BEFORE=2027 baseline', () => {
    const root = resolve(__dirname, '../../../packages/server-finance/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(2027);
    expect(total).toBe(1974);
  });
});

describe('finance read-tail r31 byte-identical read families', () => {
  const LITERALS = {
    CONSOLIDATION_RUN_BY_ID_SQL: 'SELECT * FROM finance_consolidation_runs r WHERE id=? AND holding_id=?',
    ISSUED_THAI_DOCUMENT_BY_ID_SQL: 'SELECT * FROM finance_issued_thai_documents WHERE id=? AND company_id=?',
  };

  it('exposes the two exact literals with no row-lock text', () => {
    expect(CONSOLIDATION_RUN_BY_ID_SQL).toBe(LITERALS.CONSOLIDATION_RUN_BY_ID_SQL);
    expect(ISSUED_THAI_DOCUMENT_BY_ID_SQL).toBe(LITERALS.ISSUED_THAI_DOCUMENT_BY_ID_SQL);
    for (const value of Object.values(LITERALS)) expect(value).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });

  it('removes every inline family literal from its call sites', async () => {
    for (const [file, literal] of [
      ['finance-consolidation.ts', LITERALS.CONSOLIDATION_RUN_BY_ID_SQL],
      ['finance-thai-documents.ts', LITERALS.ISSUED_THAI_DOCUMENT_BY_ID_SQL],
    ] as const) {
      const text = await readFile(new URL('../../../packages/server-finance/src/adapters/' + file, import.meta.url), 'utf8');
      expect(text.split(literal).length - 1).toBe(0);
    }
  });

  it('leaves the retained tax assertion guard inline', async () => {
    const tax = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(tax).toContain('const pendingCorrectionsSql = "SELECT COUNT(*) FROM finance_vat_corrections');
    expect(tax).toContain('`(${pendingCorrectionsSql})=0`');
  });

  it('retirement:ts-statements stays strictly below the r31 BEFORE=1989 baseline', () => {
    const root = resolve(__dirname, '../../../packages/server-finance/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(1989);
    expect(total).toBe(1974);
  });
});

describe('finance read-tail r32 byte-identical read families', () => {
  // Distinct from the earlier base_currency-only dedupe: this family returns
  // both id and base_currency from the active chart at three call sites.
  const LITERALS = {
    ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL: "SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'",
  };

  it('exposes the exact literal with no row-lock text', () => {
    expect(ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL).toBe(LITERALS.ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL);
    for (const value of Object.values(LITERALS)) expect(value).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });

  it('is not the already-deduped base_currency-only literal', () => {
    expect(ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL).not.toBe(ACTIVE_CHART_BASE_CURRENCY_SQL);
  });

  it('removes every inline family literal from its call sites', async () => {
    for (const file of ['journal-planning.ts', 'payroll-owner-facts.ts', 'finance.ts']) {
      const text = await readFile(new URL('../../../packages/server-finance/src/adapters/' + file, import.meta.url), 'utf8');
      expect(text.split(LITERALS.ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL).length - 1).toBe(0);
      expect(text).toContain('ACTIVE_CHART_BASE_CURRENCY_BY_ID_SQL');
    }
  });

  it('leaves row-lock variants inline at their adapters', async () => {
    const pos = await readFile(new URL('../../../packages/server-finance/src/adapters/pos-core-facts.ts', import.meta.url), 'utf8');
    expect(pos).toContain("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active' FOR SHARE");
  });

  it('retirement:ts-statements stays strictly below the r31 BEFORE=1985 baseline', () => {
    const root = resolve(__dirname, '../../../packages/server-finance/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(1985);
    expect(total).toBe(1974);
  });
});

describe('finance read-tail r33 byte-identical read families', () => {
  // Distinct from every r15-r32 family: the bare period point read returns the
  // full period row and is shared by three independent adapters. The longer
  // "AND state='open'" variant stays inline at its own call site.
  const LITERALS = {
    PERIOD_BY_ID_SQL: 'SELECT * FROM finance_periods WHERE id=? AND company_id=?',
  };

  it('exposes the exact literal with no row-lock text', () => {
    expect(PERIOD_BY_ID_SQL).toBe(LITERALS.PERIOD_BY_ID_SQL);
    for (const value of Object.values(LITERALS)) expect(value).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });

  it('removes every inline family literal from its call sites', async () => {
    for (const file of ['journal-planning.ts', 'finance-engines.ts', 'finance.ts']) {
      const text = await readFile(new URL('../../../packages/server-finance/src/adapters/' + file, import.meta.url), 'utf8');
      expect(text.split(`'${LITERALS.PERIOD_BY_ID_SQL}'`).length - 1).toBe(0);
      expect(text.split(`"${LITERALS.PERIOD_BY_ID_SQL}"`).length - 1).toBe(0);
      expect(text).toContain('PERIOD_BY_ID_SQL');
    }
  });

  it('leaves the longer state-scoped variant and retained guards inline', async () => {
    const finance = await readFile(new URL('../../../packages/server-finance/src/adapters/finance.ts', import.meta.url), 'utf8');
    expect(finance).toContain("SELECT * FROM finance_periods WHERE id=? AND company_id=? AND state='open'");
    const tax = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-tax-register.ts', import.meta.url), 'utf8');
    expect(tax).toContain('pendingCorrectionsSql');
  });

  it('retirement:ts-statements stays strictly below the r32 BEFORE=1983 baseline', () => {
    const root = resolve(__dirname, '../../../packages/server-finance/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(1983);
    expect(total).toBe(1974);
  });
});

describe('finance read-tail r34 byte-identical read families', () => {
  // Five families, each byte-identical at two or three independent adapters.
  // Distinct from every r15-r33 constant: the r16 EXISTS guard, the r15 DESC
  // period list, the r17 bank-transaction-by-id read and the r18 chart-id read
  // (without status) all stay inline/unchanged. Lock variants stay inline too.
  const LITERALS = {
    OPEN_PERIOD_COVERED_SQL: 'SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state=\'open\' AND start_date<=? AND end_date>=?',
    OPEN_PERIOD_LIST_ASC_SQL: 'SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state=\'open\' ORDER BY start_date',
    BANK_TRANSACTION_STATEMENT_SQL: 'SELECT t.*,s.bank_account,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.id=? AND t.company_id=?',
    ACTIVE_CHART_NAV_SQL: 'SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status=\'active\' ORDER BY name',
    ACTIVE_ACCOUNT_CHART_ID_SQL: 'SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=? AND status=\'active\'',
  };
  const CONSTANTS = {
    OPEN_PERIOD_COVERED_SQL,
    OPEN_PERIOD_LIST_ASC_SQL,
    BANK_TRANSACTION_STATEMENT_SQL,
    ACTIVE_CHART_NAV_SQL,
    ACTIVE_ACCOUNT_CHART_ID_SQL,
  };

  it('exposes the exact literals with no row-lock text', () => {
    for (const [name, value] of Object.entries(CONSTANTS)) expect(value).toBe(LITERALS[name as keyof typeof LITERALS]);
    for (const value of Object.values(LITERALS)) expect(value).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
  });

  it('removes every inline family literal from all twelve call sites', async () => {
    const sites: Record<string, string[]> = {
      OPEN_PERIOD_COVERED_SQL: ['finance-asset-policies.ts', 'finance-billing-corrections.ts', 'finance-business-contracts.ts'],
      OPEN_PERIOD_LIST_ASC_SQL: ['finance-asset-policies.ts', 'refunds.ts', 'finance-instruments.ts'],
      BANK_TRANSACTION_STATEMENT_SQL: ['finance-ai-source.ts', 'finance-invoice-read.ts'],
      ACTIVE_CHART_NAV_SQL: ['payroll-owner-facts.ts', 'finance-books.ts'],
      ACTIVE_ACCOUNT_CHART_ID_SQL: ['commerce-goods-accounting-facts.ts', 'finance-accruals.ts'],
    };
    for (const [name, files] of Object.entries(sites)) {
      const literal = LITERALS[name as keyof typeof LITERALS];
      for (const file of files) {
        const text = await readFile(new URL('../../../packages/server-finance/src/adapters/' + file, import.meta.url), 'utf8');
        expect(text.split(`'${literal}'`).length - 1).toBe(0);
        expect(text.split(`"${literal}"`).length - 1).toBe(0);
        expect(text).toContain(name);
      }
    }
  });

  it('leaves r15-r33 helpers and retained lock variants untouched', async () => {
    const r15 = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r15-sql.ts', import.meta.url), 'utf8');
    expect(r15).toContain('ORDER BY start_date DESC');
    const r16 = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r16-sql.ts', import.meta.url), 'utf8');
    expect(r16).toContain('EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state=\'open\' AND start_date<=? AND end_date>=?)');
    const r17 = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r17-sql.ts', import.meta.url), 'utf8');
    expect(r17).toContain('BANK_TRANSACTION_BY_ID_SQL');
    const r18 = await readFile(new URL('../../../packages/server-finance/src/adapters/finance-read-tail-r18-sql.ts', import.meta.url), 'utf8');
    expect(r18).toContain('SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?"');
    const pms = await readFile(new URL('../../../packages/server-finance/src/adapters/pms-command-facts.ts', import.meta.url), 'utf8');
    expect(pms).toContain('FOR SHARE');
    const pos = await readFile(new URL('../../../packages/server-finance/src/adapters/pos-core-facts.ts', import.meta.url), 'utf8');
    expect(pos).toContain('FOR UPDATE');
  });

  it('retirement:ts-statements stays strictly below the r33 BEFORE=1981 baseline', () => {
    const root = resolve(__dirname, '../../../packages/server-finance/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(root);
    expect(total).toBeLessThan(1981);
    expect(total).toBe(1974);
  });
});
