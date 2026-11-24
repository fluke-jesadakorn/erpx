import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { jsonEachSql, jsonEachApproverPoolSql } from '../../../packages/server-workflow/src/adapters/json-each-sources';
import { expenseApprovalReadBatch } from '../../../packages/server-workflow/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
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

  it('expense batch callers still emit the byte-identical json_each bind span', async () => {
    const sqls: string[] = [];
    const db = {
      prepare: (sql: string) => { sqls.push(sql); return { bind: () => ({ all: () => ({ results: [] }) }) }; },
      batch: () => { throw new Error('unexpected batch'); },
    } as unknown as SqlDatabase;
    const actor = { companyId: 'c1', userId: 'u1', role: 'staff', assignmentId: 'a1' } as unknown as Parameters<typeof expenseApprovalReadBatch>[1];
    await expenseApprovalReadBatch(db, actor, ['e1']);
    expect(sqls[0]).toBe("SELECT DISTINCT ON(record_id) * FROM approval_instances WHERE record_type='expense' AND company_id=? AND record_id IN(SELECT value FROM erpx_json_each(?)) ORDER BY record_id,insertion_order DESC");
    expect(sqls[1]).toContain('IN(SELECT value FROM erpx_json_each(?)) AND i.company_id=?');
  });

  it('keeps one generator definition and no inline value projections in callers', async () => {
    const [shared, planning] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/adapters/json-each-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/approval-planning.ts', import.meta.url), 'utf8'),
    ]);
    expect(shared.match(/export const jsonEachSql/g) ?? []).toHaveLength(1);
    expect(planning).not.toContain('SELECT value FROM erpx_json_each');
    expect(planning).toContain("jsonEachSql('?')");
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
    expect(total).toBe(55);
  });
});
