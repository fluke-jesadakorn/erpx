import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS,
  PEOPLE_PAYROLL_RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS,
  PEOPLE_PAYROLL_RD_ORG_MONTHLY_PAY_GROUPS,
  PEOPLE_PAYROLL_RD_WORK_REPORT_PEOPLE,
  PEOPLE_PAYROLL_RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE,
  PEOPLE_PAYROLL_RD_PAYROLL_OTHER_SCHEMA_AVAILABLE,
  PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME,
  PEOPLE_PAYROLL_RD_WORK_UI_HEAD_PLACEMENT,
  PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS,
  PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS,
  PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID,
} from '../../../packages/server-people_payroll/src/adapters/native-people-payroll-read';
import { expensePaymentDraftExpenseId, documentPageExpensePaymentSource } from '../../../packages/server-people_payroll/src/adapters/document-page-payment-source';
import { expensePaymentCommandFlowTarget } from '../../../packages/server-people_payroll/src/adapters/expense-command-policy';
import { attachmentMigrationpeoplepayroll } from '../../../packages/server-people_payroll/src/adapters/attachment-migration-facts';
import { createFlowRoutingSourceRepository } from '../../../packages/server-people_payroll/src/adapters/workflow-flow-routing-facts';
import { RecordDetailsPeoplePayrollOwnedFacts } from '../../../packages/server-people_payroll/src/adapters/records-record-details-owned-facts';
import {
  organizationRosterExitEngagementIds,
  organizationMonthlyPayGroups,
} from '../../../packages/server-people_payroll/src/adapters/organization-roster-facts';
import { workReportPeoplePayrollFacts } from '../../../packages/server-people_payroll/src/adapters/work-report-facts';
import { finalPayCancellationSchemaAvailable } from '../../../packages/server-people_payroll/src/adapters/final-pay-cancellation-sources';
import { payrollOtherSchemaAvailable } from '../../../packages/server-people_payroll/src/adapters/payroll-other-remittance-sources';
import { workUiStaffFacts } from '../../../packages/server-people_payroll/src/adapters/work-ui-staff-facts';

const envKey = 'ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG';
const original = process.env[envKey];
beforeEach(() => {
  process.env[envKey] = JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44993',
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
    async batch() { return [{ results: [{ id: 'run-1' }], success: true, meta: { changes: 0, duration: 0 } }]; },
    async exec() { return { count: 0, duration: 0 }; },
  };
  return { db, bound, prepared };
};

describe('native People Payroll caller-unit read plan', () => {
  it('exposes an allow-listed RD_* kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS).toBe('people_payroll_rd_record_supplement_rows');
    expect(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the record-supplement read and runs it on the caller unit with its binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    const result = await RecordDetailsPeoplePayrollOwnedFacts(db as never)
      .readRecordSupplementDataRows4('company', '2026-11', 'THB', 30);
    expect(result).toMatchObject({ results: [{ id: 'run-1' }] });

    expect(bodies).toEqual([{
      kind: PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS,
      values: ['company', '2026-11', 'THB', 30],
    }]);
    expect(bound).toEqual([['company', '2026-11', 'THB', 30]]);
    expect(prepared).toEqual(['SELECT 1 AS row']);

    const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/people-payroll/command-plan');
    expect((init.headers as Record<string, string>).origin).toBe('https://acme.example.com');
    expect((init.headers as Record<string, string>)['x-erpx-context-company']).toBe('company');
  });

  it('exposes organization roster read kinds with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS).toBe('people_payroll_rd_org_roster_exit_engagement_ids');
    expect(PEOPLE_PAYROLL_RD_ORG_MONTHLY_PAY_GROUPS).toBe('people_payroll_rd_org_monthly_pay_groups');
    for (const kind of [PEOPLE_PAYROLL_RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS, PEOPLE_PAYROLL_RD_ORG_MONTHLY_PAY_GROUPS]) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
    }
  });

  it('plans the organization roster read family on the caller unit with its binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    const exits = await organizationRosterExitEngagementIds(db as never, 'company', 'position-1');
    const groups = await organizationMonthlyPayGroups(db as never, 'company');
    expect(exits).toEqual(['run-1']);
    expect(groups).toEqual([{ id: 'run-1' }]);

    expect(bodies).toEqual([
      { kind: PEOPLE_PAYROLL_RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS, values: ['company', 'position-1'] },
      { kind: PEOPLE_PAYROLL_RD_ORG_MONTHLY_PAY_GROUPS, values: ['company'] },
    ]);
    expect(bound).toEqual([['company', 'position-1'], ['company']]);
    expect(prepared).toEqual(['SELECT 1 AS row', 'SELECT 1 AS row']);
  });

  it('exposes the work-report people read kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_WORK_REPORT_PEOPLE).toBe('people_payroll_rd_work_report_people');
    expect(PEOPLE_PAYROLL_RD_WORK_REPORT_PEOPLE).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the work-report people read on the caller unit with its bind', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    const people = await workReportPeoplePayrollFacts(db as never).people('company');
    expect(people).toMatchObject({ results: [{ id: 'run-1' }] });

    expect(bodies).toEqual([{
      kind: PEOPLE_PAYROLL_RD_WORK_REPORT_PEOPLE,
      values: ['company'],
    }]);
    expect(bound).toEqual([['company']]);
    expect(prepared).toEqual(['SELECT 1 AS row']);
  });

  it('exposes the final-pay schema-probe read kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE).toBe('people_payroll_rd_final_pay_cancellation_schema_available');
    expect(PEOPLE_PAYROLL_RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the zero-bind final-pay schema probe on the caller unit and reads its boolean', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS ready', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const prepared: string[] = [];
    const db = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind() { return this; },
          async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
          async first() { return null; },
        };
      },
      async batch() { return [{ results: [{ ready: true }], success: true, meta: { changes: 0, duration: 0 } }]; },
      async exec() { return { count: 0, duration: 0 }; },
    };

    expect(await finalPayCancellationSchemaAvailable(db as never)).toBe(true);

    expect(bodies).toEqual([{
      kind: PEOPLE_PAYROLL_RD_FINAL_PAY_CANCELLATION_SCHEMA_AVAILABLE,
      values: [],
    }]);
    expect(prepared).toEqual(['SELECT 1 AS ready']);
  });

  it('exposes the Payroll-other schema-probe read kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_PAYROLL_OTHER_SCHEMA_AVAILABLE).toBe('people_payroll_rd_payroll_other_schema_available');
    expect(PEOPLE_PAYROLL_RD_PAYROLL_OTHER_SCHEMA_AVAILABLE).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the zero-bind Payroll-other schema probe on the caller unit and reads its row presence', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const prepared: string[] = [];
    let rows: unknown[] = [{ '?column?': 1 }];
    const db = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind() { return this; },
          async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
          async first() { return null; },
        };
      },
      async batch() { return [{ results: rows, success: true, meta: { changes: 0, duration: 0 } }]; },
      async exec() { return { count: 0, duration: 0 }; },
    };

    expect(await payrollOtherSchemaAvailable(db as never)).toBe(true);
    rows = [];
    expect(await payrollOtherSchemaAvailable(db as never)).toBe(false);

    expect(bodies).toEqual([
      { kind: PEOPLE_PAYROLL_RD_PAYROLL_OTHER_SCHEMA_AVAILABLE, values: [] },
      { kind: PEOPLE_PAYROLL_RD_PAYROLL_OTHER_SCHEMA_AVAILABLE, values: [] },
    ]);
    expect(prepared).toEqual(['SELECT 1', 'SELECT 1']);
  });

  it('exposes the flow-routing legal-name read kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME).toBe('people_payroll_rd_flow_routing_person_legal_name');
    expect(PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the deduped flow-routing legal-name reads on the caller unit with their binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const prepared: string[] = [];
    const bound: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind(...values: unknown[]) { bound.push(values); return this; },
          async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
          async first() { return null; },
        };
      },
      async batch() { return [{ results: [{ legal_name: 'Alice' }], success: true, meta: { changes: 0, duration: 0 } }]; },
      async exec() { return { count: 0, duration: 0 }; },
    };

    const repo = createFlowRoutingSourceRepository(db as never);
    expect(await repo.readPrepareFlowCommandPerson('company', 'user-1')).toEqual({ legal_name: 'Alice' });
    expect(await repo.readAssignFlowPrimaryAdministrator('company', 'user-2')).toEqual({ legal_name: 'Alice' });

    expect(bodies).toEqual([
      { kind: PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME, values: ['company', 'user-1'] },
      { kind: PEOPLE_PAYROLL_RD_FLOW_ROUTING_PERSON_LEGAL_NAME, values: ['company', 'user-2'] },
    ]);
    expect(bound).toEqual([['company', 'user-1'], ['company', 'user-2']]);
    expect(prepared).toEqual(['SELECT 1 AS row', 'SELECT 1 AS row']);
  });

  it('exposes the work-UI head-placement read kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_WORK_UI_HEAD_PLACEMENT).toBe('people_payroll_rd_work_ui_head_placement');
    expect(PEOPLE_PAYROLL_RD_WORK_UI_HEAD_PLACEMENT).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the work-UI head-placement read on the caller unit with its binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    const placement = await workUiStaffFacts(db as never).headPlacement('company', 'department-1', 'person-1');
    expect(placement).toEqual({ id: 'run-1' });

    expect(bodies).toEqual([{
      kind: PEOPLE_PAYROLL_RD_WORK_UI_HEAD_PLACEMENT,
      values: ['company', 'department-1', 'person-1'],
    }]);
    expect(bound).toEqual([['company', 'department-1', 'person-1']]);
    expect(prepared).toEqual(['SELECT 1 AS row']);
  });

  it('exposes attachment-storage reconciliation read kinds with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS).toBe('people_payroll_rd_attachment_migration_payment_exports');
    expect(PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS).toBe('people_payroll_rd_attachment_migration_payroll_review_exports');
    for (const kind of [PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS, PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS]) {
      expect(kind).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
    }
  });

  it('plans both zero-bind attachment reconciliation reads on the caller unit', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS row', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const { db, bound, prepared } = fakeDb();

    const facts = attachmentMigrationpeoplepayroll(db as never);
    const exports = await facts.readReconcileAttachmentStoragePaymentExports();
    const reviews = await facts.readReconcileAttachmentStoragePayrollReviewExports();
    expect(exports).toMatchObject({ results: [{ id: 'run-1' }] });
    expect(reviews).toMatchObject({ results: [{ id: 'run-1' }] });

    expect(bodies).toEqual([
      { kind: PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYMENT_EXPORTS, values: [] },
      { kind: PEOPLE_PAYROLL_RD_ATTACHMENT_MIGRATION_PAYROLL_REVIEW_EXPORTS, values: [] },
    ]);
    expect(bound).toEqual([[], []]);
    expect(prepared).toEqual(['SELECT 1 AS row', 'SELECT 1 AS row']);
  });

  it('exposes the deduped expense-payment-draft expense-id read kind with no SQL on the wire', () => {
    expect(PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID).toBe('people_payroll_rd_expense_payment_draft_expense_id');
    expect(PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID).not.toMatch(/SELECT|INSERT|UPDATE|DELETE/);
  });

  it('plans the deduped expense-payment-draft expense-id reads on the caller unit with their binds', async () => {
    const bodies: { kind: string; values: unknown[] }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (_url: URL, init: RequestInit) => {
      const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
      bodies.push({ kind: body.kind, values: body.values });
      return new Response(JSON.stringify({
        data: { statements: [{ sql: 'SELECT 1 AS expense_id', values: body.values }] },
      }), { status: 200, headers: { 'content-type': 'application/json' } });
    }));
    const prepared: string[] = [];
    const bound: unknown[][] = [];
    const db = {
      prepare(sql: string) {
        prepared.push(sql);
        return {
          bind(...values: unknown[]) { bound.push(values); return this; },
          async all() { return { results: [], success: true, meta: { changes: 0, duration: 0 } }; },
          async first() { return null; },
        };
      },
      async batch() { return [{ results: [{ expense_id: 'exp-1' }], success: true, meta: { changes: 0, duration: 0 } }]; },
      async exec() { return { count: 0, duration: 0 }; },
    };

    const canonical = await expensePaymentDraftExpenseId(db as never, 'draft-1', 'company');
    const page = await documentPageExpensePaymentSource(db as never, 'draft-2', 'company');
    const commandTarget = await expensePaymentCommandFlowTarget(
      db as never, { companyId: 'company' } as never, 'expense.payment.release', 'draft-3',
    );
    expect(canonical).toEqual({ expense_id: 'exp-1' });
    expect(page).toEqual({ expense_id: 'exp-1' });
    expect(commandTarget).toEqual({ kind: 'expense', id: 'exp-1', action: 'payment' });

    expect(bodies).toEqual([
      { kind: PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID, values: ['draft-1', 'company'] },
      { kind: PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID, values: ['draft-2', 'company'] },
      { kind: PEOPLE_PAYROLL_RD_EXPENSE_PAYMENT_DRAFT_EXPENSE_ID, values: ['draft-3', 'company'] },
    ]);
    expect(bound).toEqual([['draft-1', 'company'], ['draft-2', 'company'], ['draft-3', 'company']]);
    expect(prepared).toEqual(['SELECT 1 AS expense_id', 'SELECT 1 AS expense_id', 'SELECT 1 AS expense_id']);
  });

  it('fails closed when configuration is missing or the plan is malformed', async () => {
    delete process.env[envKey];
    const { db, prepared } = fakeDb();
    await expect(nativePeoplePayrollReadAll(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS, ['company', '2026-11', 'THB', 0], db as never))
      .rejects.toMatchObject({ code: 'PEOPLE_PAYROLL_NATIVE_READ_UNAVAILABLE' });
    expect(prepared).toEqual([]);

    process.env[envKey] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:44993', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001', companyId: 'company' }],
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(JSON.stringify({ data: {} }), {
      status: 200, headers: { 'content-type': 'application/json' },
    })));
    await expect(nativePeoplePayrollReadAll(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS, ['company', '2026-11', 'THB', 0], db as never))
      .rejects.toMatchObject({ code: 'PEOPLE_PAYROLL_NATIVE_READ_UNAVAILABLE' });
  });
});
