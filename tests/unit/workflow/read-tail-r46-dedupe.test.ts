import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi, beforeEach } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('../../../packages/server-workflow/src/adapters/native-workflow-read', () => ({
  nativeWorkflowReadAll: vi.fn(async () => []),
  nativeWorkflowReadFirst: vi.fn(async () => null),
}));
import { nativeWorkflowReadAll } from '../../../packages/server-workflow/src/adapters/native-workflow-read';
import { nativeApprovalPlanningPorts, expenseApprovalReadBatch } from '../../../packages/server-workflow/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const readAll = vi.mocked(nativeWorkflowReadAll);
const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');

/** Independent reconstruction of the three retired TypeScript literals (r46). */
const hr = '"id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone"';
const stepsPrefix =
  `SELECT s.*,COALESCE(p.legal_name,s.approver_user_id) approver_name FROM approval_steps s LEFT JOIN (SELECT ${hr} FROM hr_people) p ON p.user_id=s.approver_user_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE s.instance_id`;
const approverSql = `${stepsPrefix}=? ORDER BY s.ordinal`;
const stepsBatchSql = `${stepsPrefix} IN(SELECT value FROM erpx_json_each(?)) ORDER BY s.ordinal`;
const instancesSql =
  "SELECT DISTINCT ON(record_id) * FROM approval_instances WHERE record_type='expense' AND company_id=? AND record_id IN(SELECT value FROM erpx_json_each(?)) ORDER BY record_id,insertion_order DESC";
const rustLiteral = (sql: string) => sql.replaceAll('"', '\\"');

function capture() {
  const calls: { sql: string; binds: unknown[] }[] = [];
  const stmt = (sql: string) => ({
    bind(...values: unknown[]) { calls.push({ sql, binds: values }); return this; },
    first() { return undefined; },
    all() { return { results: [] as unknown[] }; },
  });
  const db = { prepare: (sql: string) => stmt(sql) } as unknown as SqlDatabase;
  return { db, calls };
}

beforeEach(() => {
  readAll.mockReset();
  readAll.mockResolvedValue([]);
});

describe('approval-step and expense-batch native cutover (r46)', () => {
  it('registers byte-identical, SELECT-only SQL with two binds for each kind', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/domain/workflow_wave3.rs', import.meta.url), 'utf8');
    for (const [kind, sql] of [
      ['wf_native_approval_steps_approver', approverSql],
      ['wf_native_approval_steps_batch', stepsBatchSql],
      ['wf_native_expense_batch_instances', instancesSql],
    ] as const) {
      expect(src).toContain(`"${kind}"`);
      expect(src).toContain(rustLiteral(sql));
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect((sql.match(/\?/g) ?? []).length).toBe(2);
      expect(sql).toMatch(/approval_steps|approval_instances/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|RETURNING|;|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  it('readStepsForEffect emits no local SQL and keeps its exact binds', async () => {
    const { db, calls } = capture();
    const repo = nativeApprovalPlanningPorts(db).repository as unknown as {
      readStepsForEffect: (company: string, id: string) => Promise<{ results: unknown[] }>;
    };
    await expect(repo.readStepsForEffect('co-1', 'inst-1')).resolves.toEqual({ results: [] });
    expect(calls).toEqual([]);
    expect(readAll).toHaveBeenCalledWith(
      'wf_native_approval_steps_approver', ['co-1', 'inst-1'], 'co-1', db,
    );
  });

  it('expenseApprovalReadBatch uses the native batch kinds with unchanged binds', async () => {
    readAll.mockImplementation(async (kind: string) => {
      if (kind === 'wf_native_expense_batch_instances')
        return [{ record_id: 'e1', id: 'inst-1', state: 'pending' }] as never;
      return [] as never;
    });
    const { db, calls } = capture();
    const views = await expenseApprovalReadBatch(
      db, { companyId: 'co-1', userId: 'u', role: 'staff', assignmentId: 'a' }, ['e1'],
    );
    expect(typeof views).toBe('function');
    expect(calls).toEqual([]);
    expect(readAll).toHaveBeenCalledWith(
      'wf_native_expense_batch_instances', ['co-1', '["e1"]'], 'co-1', db,
    );
    expect(readAll).toHaveBeenCalledWith(
      'wf_native_approval_steps_batch', ['co-1', '["inst-1"]'], 'co-1', db,
    );
    expect(readAll).toHaveBeenCalledWith(
      'wf_native_approval_view_history_batch', ['["e1"]', 'co-1'], 'co-1', db,
    );
  });

  it('removes the retired step-approver and expense-batch SELECT literals', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/adapters/approval-planning.ts', import.meta.url), 'utf8');
    expect(src).not.toContain('COALESCE(p.legal_name,s.approver_user_id) approver_name');
    expect(src).not.toContain('SELECT DISTINCT ON(record_id) * FROM approval_instances');
    expect(src).not.toContain('approvalStepsApproverSql');
    expect(src).toContain("'wf_native_approval_steps_approver'");
    expect(src).toContain("'wf_native_approval_steps_batch'");
    expect(src).toContain("'wf_native_expense_batch_instances'");
  });

  it('retirement:ts-statements is 33 after the r46/r47 cutover and r48 dedupe (43->41->37->33)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(33);
  });
});
