import { readFile } from 'node:fs/promises';
import { describe, it, expect, vi, beforeEach } from 'vitest';
vi.mock('server-only', () => ({}));
vi.mock('../../../packages/server-workflow/src/adapters/native-workflow-read', () => ({
  nativeWorkflowReadAll: vi.fn(async () => []),
  nativeWorkflowReadFirst: vi.fn(async () => null),
}));
import { nativeWorkflowReadAll, nativeWorkflowReadFirst } from '../../../packages/server-workflow/src/adapters/native-workflow-read';
import { nativeApprovalPlanningPorts, expenseApprovalReadBatch } from '../../../packages/server-workflow/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const readAll = vi.mocked(nativeWorkflowReadAll);
const readFirst = vi.mocked(nativeWorkflowReadFirst);

/** The retired TypeScript owner projection, reconstructed here so byte
 * identity is checked against an independent literal, not the registry copy. */
const hr = '"id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone"';
const nameLookup = (column: string) =>
  `(SELECT legal_name FROM (SELECT ${hr} FROM hr_people) hr_people WHERE company_id=i.company_id AND iam_same_person(user_id,d.${column},company_id) AND merged_into_id IS NULL LIMIT 1)`;
const historyProjection =
  `COALESCE(p.legal_name,d.actor_user_id) actor_name,${nameLookup('previous_approver_user_id')} previous_approver_name,${nameLookup('approver_user_id')} approver_name FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id LEFT JOIN (SELECT ${hr} FROM hr_people) p ON p.user_id=d.actor_user_id AND p.company_id=i.company_id AND p.merged_into_id IS NULL`;
const singleHistorySql =
  `SELECT d.*,${historyProjection} WHERE i.record_type=? AND i.record_id=? AND i.company_id=? ORDER BY d.created_at,d.insertion_order`;
const batchHistorySql =
  `SELECT i.record_id batch_record_id,d.*,${historyProjection} WHERE i.record_type='expense' AND i.record_id IN(SELECT value FROM erpx_json_each(?)) AND i.company_id=? ORDER BY d.created_at,d.insertion_order`;
const previousCycleSql =
  'SELECT COALESCE(MAX(cycle),0) cycle FROM approval_instances WHERE record_type=? AND record_id=? AND phase=?';
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
  readAll.mockClear();
  readFirst.mockClear();
  readAll.mockResolvedValue([]);
  readFirst.mockResolvedValue(null);
});

describe('approval point-read native cutover (r44)', () => {
  it('registers byte-identical, SELECT-only SQL with unchanged binds', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/domain/workflow_wave3.rs', import.meta.url), 'utf8');
    for (const [sql, binds] of [
      [previousCycleSql, 3],
      [singleHistorySql, 3],
      [batchHistorySql, 2],
    ] as const) {
      expect(src).toContain(rustLiteral(sql));
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect((sql.match(/\?/g) ?? []).length).toBe(binds);
      expect(sql).toMatch(/approval_instances|approval_decisions/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|RETURNING|;|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  it('readOpenApprovalPrevious emits no local SQL and keeps its exact binds', async () => {
    const { db, calls } = capture();
    const repo = nativeApprovalPlanningPorts(db).repository as unknown as {
      readOpenApprovalPrevious: (kind: string, id: string, phase: string, companyId: string) => Promise<unknown>;
    };
    await expect(repo.readOpenApprovalPrevious('expense', 'rec-1', 'start', 'co-1')).resolves.toBeNull();
    expect(calls).toEqual([]);
    expect(readFirst).toHaveBeenCalledWith('wf_native_open_approval_previous', ['expense', 'rec-1', 'start'], 'co-1', db);
    expect(readAll).not.toHaveBeenCalled();
  });

  it('readApprovalViewHistory emits no local SQL and keeps its exact binds', async () => {
    const { db, calls } = capture();
    const repo = nativeApprovalPlanningPorts(db).repository as unknown as {
      readApprovalViewHistory: (kind: string, id: string, companyId: string) => Promise<unknown>;
    };
    await expect(repo.readApprovalViewHistory('leave', 'rec-2', 'co-2')).resolves.toEqual({ results: [] });
    expect(calls).toEqual([]);
    expect(readAll).toHaveBeenCalledWith('wf_native_approval_view_history', ['leave', 'rec-2', 'co-2'], 'co-2', db);
    expect(readFirst).not.toHaveBeenCalled();
  });

  it('expense batch history uses the native batch kind with unchanged binds', async () => {
    const { db } = capture();
    const views = await expenseApprovalReadBatch(db, { companyId: 'co-3', userId: 'u', role: 'staff', assignmentId: 'a' }, ['rec-3']);
    expect(typeof views).toBe('function');
    expect(readAll).toHaveBeenCalledWith('wf_native_approval_view_history_batch', ['["rec-3"]', 'co-3'], 'co-3', db);
  });

  it('removes all three retired point-read SELECT literals from the adapter', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/adapters/approval-planning.ts', import.meta.url), 'utf8');
    expect(src).not.toContain('SELECT COALESCE(MAX(cycle),0) cycle FROM approval_instances');
    expect(src).not.toContain('COALESCE(p.legal_name,d.actor_user_id) actor_name');
    expect(src).not.toContain('batch_record_id,d.*');
    expect(src).toContain("'wf_native_open_approval_previous'");
    expect(src).toContain("'wf_native_approval_view_history'");
    expect(src).toContain("'wf_native_approval_view_history_batch'");
  });
});
