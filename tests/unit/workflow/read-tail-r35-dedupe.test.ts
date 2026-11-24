import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { jsonEachApproverPoolSql } from '../../../packages/server-workflow/src/adapters/json-each-sources';
import { participantSql } from '../../../packages/server-workflow/src/adapters/approval-participant-projection';
import { nativeApprovalPlanningPorts } from '../../../packages/server-workflow/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const poolSql = (expr: string) => `SELECT value FROM erpx_json_each(COALESCE(${expr},'[]'))`;

// Guards are built synchronously; no DB execution occurs.
const db = { batch: () => { throw new Error('unexpected batch'); } } as unknown as SqlDatabase;
const guards = (nativeApprovalPlanningPorts(db).repository as unknown as {
  guard31: (...args: unknown[]) => { kind: string; values: unknown[] };
});

describe('approver-pool erpx_json_each generator family (r35)', () => {
  it('emits byte-identical SQL for both pre-dedupe literals', () => {
    expect(jsonEachApproverPoolSql('approver_pool_json')).toBe(
      "SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))",
    );
    expect(jsonEachApproverPoolSql('ast.approver_pool_json')).toBe(
      "SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))",
    );
    expect(jsonEachApproverPoolSql('ast.approver_pool_json')).toBe(poolSql('ast.approver_pool_json'));
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const sql = jsonEachApproverPoolSql('approver_pool_json');
    expect(sql).toContain('SELECT ');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('participantSql keeps its full emitted span byte-identical', () => {
    const sql = participantSql('expense', 'd');
    expect(sql).toContain("UNION SELECT value FROM erpx_json_each(COALESCE(ast.approver_pool_json,'[]'))) participant");
    expect(sql.match(/erpx_json_each\(COALESCE\(/g) ?? []).toHaveLength(1);
  });

  it('guard31 (non-override) keeps its full emitted span byte-identical', () => {
    const stmt = guards.guard31('key', 'approval-version', 'id', 1, 0, 'id2', 'user', false);
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values[7]).toContain(
      "UNION SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))) AND state='pending')",
    );
  });

  it('keeps one generator definition and no inline COALESCE literals in callers', async () => {
    const [shared, planning, participant] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/adapters/json-each-sources.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/approval-planning.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/approval-participant-projection.ts', import.meta.url), 'utf8'),
    ]);
    expect(shared.match(/export const jsonEachApproverPoolSql/g) ?? []).toHaveLength(1);
    expect(planning).not.toContain("erpx_json_each(COALESCE(approver_pool_json");
    expect(planning).toContain("approverUnionProjectionSql('plain')");
    expect(participant).not.toContain("erpx_json_each(COALESCE(ast.approver_pool_json");
    expect(participant).toContain("approverUnionProjectionSql('step')");
  });

  it('retirement:ts-statements stays strictly below the recorded 58 r34 baseline (r36 lowers it further)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(58);
    expect(total).toBeLessThanOrEqual(56);
  });
});
