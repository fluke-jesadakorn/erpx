import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('../../../packages/server-workflow/src/adapters/native-workflow-read', () => ({
  nativeWorkflowReadAll: vi.fn(async () => []),
  nativeWorkflowReadFirst: vi.fn(async () => null),
}));
import { tableExistsGuardSql } from '../../../packages/server-workflow/src/adapters/closed-projection-sources';
import { nativeApprovalPlanningPorts } from '../../../packages/server-workflow/src/adapters/approval-planning';
import { nativeFlowPlanningPorts } from '../../../packages/server-workflow/src/adapters/flow-planning-ports';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const src = (rel: string) => readFile(resolve(srcRoot, rel), 'utf8');
// Guards build assertion descriptors synchronously; no SQL is executed.
const db = { batch: () => { throw new Error('unexpected batch'); } } as unknown as SqlDatabase;
const approval = nativeApprovalPlanningPorts(db).repository as unknown as {
  guard17: (...args: unknown[]) => { kind: string; values: unknown[] };
  guard49: (...args: unknown[]) => { kind: string; values: unknown[] };
};
const flow = nativeFlowPlanningPorts(db).repository as unknown as {
  guard24: (...args: unknown[]) => { kind: string; values: unknown[] };
  guard34: (...args: unknown[]) => { kind: string; values: unknown[] };
};

// Byte oracles captured from HEAD ce4489060 before the r50 fold. The two wrappers
// keep their names, bind order and exact emitted SQL after delegating the shared head.
const pending = "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?)";
const expenseRecord = (versionFirst: boolean) =>
  `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND ${versionFirst ? 'version=? AND status=?' : 'status=? AND version=?'})`;
const routeOracle = 'EXISTS(SELECT 1 FROM flow_routes WHERE company_id=? AND kind=? AND record_id=? AND version=?)';
const documentOracle = (table: string) => `EXISTS(SELECT 1 FROM ${table} WHERE company_id=? AND id=? AND version=?)`;
const documentTables = [
  ['expense', 'finance_expenses'],
  ['leave', 'leave_requests'],
  ['staff_task', 'staff_tasks'],
  ['request', 'employee_requests'],
  ['stock_count', 'inventory_counts'],
  ['transfer', 'inventory_transfers'],
  ['order', 'commerce_orders'],
  ['invoice', 'finance_invoices'],
  ['journal', 'finance_journals'],
  ['payroll', 'hr_payroll_runs'],
  ['record', 'workspace_records'],
] as const;

describe('read-tail r50 table-EXISTS head fold', () => {
  it('renders the shared head byte-identically for both tail shapes', () => {
    expect(tableExistsGuardSql('finance_expenses', 'id=? AND company_id=? AND status=? AND version=?)')).toBe(expenseRecord(false));
    expect(tableExistsGuardSql('flow_routes', 'company_id=? AND kind=? AND record_id=? AND version=?)')).toBe(routeOracle);
    expect(tableExistsGuardSql('finance_expenses', 'company_id=? AND id=? AND version=?)')).toBe(documentOracle('finance_expenses'));
    for (const [, table] of documentTables) {
      expect(tableExistsGuardSql(table, 'company_id=? AND id=? AND version=?)')).toBe(documentOracle(table));
    }
  });

  it('keeps approvalRecordVersionExists byte-identical in guard17 and guard49', () => {
    const legacy = approval.guard17('key', 'legacy-record', 'rec', 'co', 'pending_approval', 3, 'expense');
    expect(legacy.kind).toBe('wf_native_command_assertions_3');
    expect(legacy.values).toEqual(['key', 'legacy-record', 'rec', 'co', 'pending_approval', 3, expenseRecord(false)]);
    const reroute = approval.guard49('key', 'route-version', 'inst', 5, 2, 'rec', 'co', 9, 'pending', 'expense');
    expect(reroute.kind).toBe('wf_native_command_assertions_3');
    expect(reroute.values).toEqual(['key', 'route-version', 'inst', 5, 2, 'rec', 'co', 9, 'pending', pending + ' AND ' + expenseRecord(true)]);
  });

  it('keeps flowVersionGuardSql byte-identical for the route and all 11 kinds', () => {
    const route = flow.guard24('key', 'route-version', ['co', 'kind', 'rec'], 9);
    expect(route.kind).toBe('wf_native_command_assertions_3');
    expect(route.values).toEqual(['key', 'route-version', 'co', 'kind', 'rec', 9, routeOracle]);
    let n = 0;
    for (const [kind, table] of documentTables) {
      const stmt = flow.guard34('key', 'flow-document-version', 'co', 'rec', 4, kind);
      expect(stmt.kind).toBe('wf_native_command_assertions_3');
      expect(stmt.values).toEqual(['key', 'flow-document-version', 'co', 'rec', 4, documentOracle(table)]);
      n += 1;
    }
    expect(n).toBe(11);
  });

  it('folds one head literal into closed-projection-sources and delegates both wrappers', async () => {
    const [shared, approvalSrc, flowSrc] = await Promise.all([
      src('adapters/closed-projection-sources.ts'),
      src('adapters/approval-planning.ts'),
      src('adapters/flow-planning-ports.ts'),
    ]);
    const head = /EXISTS\(SELECT 1 FROM \$\{table\} WHERE /g;
    expect(shared.match(head) ?? []).toHaveLength(1);
    expect(approvalSrc.match(head) ?? []).toHaveLength(0);
    expect(flowSrc.match(head) ?? []).toHaveLength(0);
    expect(shared.match(/export const tableExistsGuardSql =/g) ?? []).toHaveLength(1);
    expect(approvalSrc.match(/tableExistsGuardSql\(/g) ?? []).toHaveLength(1);
    expect(flowSrc.match(/tableExistsGuardSql\(/g) ?? []).toHaveLength(1);
    expect(approvalSrc.match(/approvalRecordVersionExists\(/g) ?? []).toHaveLength(2);
    expect(flowSrc.match(/flowVersionGuardSql\(/g) ?? []).toHaveLength(2);
    expect(approvalSrc).toContain('tableExistsGuardSql(table, `id=? AND company_id=? AND ');
    expect(flowSrc).toContain('tableExistsGuardSql(table, `company_id=? AND ${key} AND version=?)`)');
  });

  it('folds only read-only guards: no writes, locks, native kinds or cross-owner reads', () => {
    const guards = [
      expenseRecord(false), expenseRecord(true), routeOracle,
      ...documentTables.map(([, table]) => documentOracle(table)),
    ];
    for (const sql of guards) {
      expect(sql).toMatch(/^EXISTS\(SELECT 1 FROM \w+ WHERE /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativeWorkflow/i);
    }
  });

  it('retirement:ts-statements is 26 after the r39 native inventory cut the r53 head fold (29->28)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(30);
    expect(total).toBe(26);
  });
});
