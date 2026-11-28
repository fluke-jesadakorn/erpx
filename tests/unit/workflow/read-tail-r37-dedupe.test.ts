import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
// r44 moved the batch approval-history read onto the mounted native read; the
// r37 generator assertions below still cover the retained json_each bind span.
vi.mock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
  nativeWorkflowReadAll: vi.fn(async () => []),
  nativeWorkflowReadFirst: vi.fn(async () => null),
}));
import { nativeWorkflowReadAll } from '../../../modules/workflow/server/src/adapters/native-workflow-read';
import { jsonEachSql, jsonEachApproverPoolSql } from '../../../modules/workflow/server/src/adapters/json-each-sources';
import { expenseApprovalReadBatch } from '../../../modules/workflow/server/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../modules/workflow/server/src');
const valueSql = (expr: string) => `SELECT value FROM erpx_json_each(${expr})`;

describe('erpx_json_each value generator family (r37)', () => {
  it('jsonEachSql emits byte-identical SQL for the bind and COALESCE expressions', () => {
    expect(jsonEachSql('?')).toBe(valueSql('?'));
    expect(jsonEachSql(`COALESCE(approver_pool_json,'[]')`)).toBe(valueSql(`COALESCE(approver_pool_json,'[]')`));
  });

  it('jsonEachApproverPoolSql keeps its r35 emitted spans byte-identical', () => {
    expect(jsonEachApproverPoolSql('approver_pool_json')).toBe(
      "SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))",
    );
    expect(jsonEachApproverPoolSql('ast.approver_pool_json')).toBe(valueSql(`COALESCE(ast.approver_pool_json,'[]')`));
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const sql = jsonEachSql('?');
    expect(sql).toContain('SELECT ');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('expense batch callers emit no local SQL and keep the exact json_each span in the owner registry', async () => {
    const sqls: string[] = [];
    const db = {
      prepare: (sql: string) => { sqls.push(sql); return { bind: () => ({ all: () => ({ results: [] }) }) }; },
      batch: () => { throw new Error('unexpected batch'); },
    } as unknown as SqlDatabase;
    const actor = { companyId: 'c1', userId: 'u1', role: 'staff', assignmentId: 'a1' } as unknown as Parameters<typeof expenseApprovalReadBatch>[1];
    await expenseApprovalReadBatch(db, actor, ['e1']);
    expect(sqls).toHaveLength(0);
    // Both batch reads are native now; the exact json_each spans live in the
    // owner registry and the bridge carries only the typed kinds and binds.
    expect(vi.mocked(nativeWorkflowReadAll)).toHaveBeenCalledWith(
      'wf_native_expense_batch_instances', ['c1', '["e1"]'], 'c1', db,
    );
    expect(vi.mocked(nativeWorkflowReadAll)).toHaveBeenCalledWith(
      'wf_native_approval_view_history_batch', ['["e1"]', 'c1'], 'c1', db,
    );
    const registry = await readFile(new URL('../../../modules/workflow/server/src/domain/workflow_wave3.rs', import.meta.url), 'utf8');
    expect(registry).toContain('i.record_id IN(SELECT value FROM erpx_json_each(?)) AND i.company_id=?');
  });

  it('keeps one generator definition and no inline value projections in callers', async () => {
    const [shared, planning] = await Promise.all([
      readFile(new URL('../../../modules/workflow/server/src/adapters/json-each-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/workflow/server/src/adapters/approval-planning.ts', import.meta.url), 'utf8'),
    ]);
    expect(shared.match(/export const jsonEachSql/g) ?? []).toHaveLength(1);
    expect(planning).not.toContain('SELECT value FROM erpx_json_each');
    expect(planning).toContain("'wf_native_expense_batch_instances'");
  });

  it('retirement:ts-statements is strictly lower than the recorded 56 r36 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(56);
    expect(total).toBeLessThanOrEqual(54);
  });
});
