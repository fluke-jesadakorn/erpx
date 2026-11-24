import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeFlowPlanningPorts } from '../../../packages/server-workflow/src/adapters/flow-planning-ports';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const historySql = (columns: string, extraWhere = ''): string =>
  `SELECT ${columns} FROM flow_events WHERE company_id=?${extraWhere} ORDER BY created_at,id`;

function capture() {
  const calls: { sql: string; binds: unknown[] }[] = [];
  const stmt = (sql: string) => ({
    bind(...values: unknown[]) { calls.push({ sql, binds: values }); return this; },
    all() { return { results: [] as unknown[] }; },
    first() { return undefined; },
  });
  const db = { prepare: (sql: string) => stmt(sql) } as unknown as SqlDatabase;
  return { db, calls };
}

describe('company-scoped flow-event history generator family (r38)', () => {
  it('emits byte-identical SQL for both pre-dedupe projection literals', () => {
    expect(historySql('kind,record_id,action,actor_name,created_at,reason,step_key')).toBe(
      'SELECT kind,record_id,action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? ORDER BY created_at,id',
    );
    expect(historySql('action,actor_name,created_at,reason,step_key', ' AND kind=? AND record_id=?')).toBe(
      'SELECT action,actor_name,created_at,reason,step_key FROM flow_events WHERE company_id=? AND kind=? AND record_id=? ORDER BY created_at,id',
    );
  });

  it('keeps the fragment read-only (no writes or row locks)', () => {
    const sql = historySql('*');
    expect(sql).toContain('SELECT ');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('both repository reads still emit byte-identical SQL and bind order', () => {
    const { db, calls } = capture();
    const repo = nativeFlowPlanningPorts(db).repository as unknown as {
      readSharedFlowReferencesRoutesdefaultsevents3: (companyId: string) => unknown;
      readReadFlowHistoryRows: (companyId: string, kind: string, id: string) => unknown;
    };
    repo.readSharedFlowReferencesRoutesdefaultsevents3('co-1');
    repo.readReadFlowHistoryRows('co-2', 'expense', 'rec-1');
    expect(calls).toEqual([
      { sql: historySql('kind,record_id,action,actor_name,created_at,reason,step_key'), binds: ['co-1'] },
      { sql: historySql('action,actor_name,created_at,reason,step_key', ' AND kind=? AND record_id=?'), binds: ['co-2', 'expense', 'rec-1'] },
    ]);
  });

  it('keeps one generator definition and no inline flow_events history literals in callers', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/adapters/flow-planning-ports.ts', import.meta.url), 'utf8');
    expect(src.match(/const flowEventsHistorySql =/g) ?? []).toHaveLength(1);
    expect(src.match(/FROM flow_events WHERE company_id=\?/g) ?? []).toHaveLength(1);
    expect(src).not.toContain('SELECT kind,record_id,action,actor_name,created_at,reason,step_key FROM flow_events');
    expect(src).not.toContain('SELECT action,actor_name,created_at,reason,step_key FROM flow_events');
    expect(src).toContain("flowEventsHistorySql('kind,record_id,action,actor_name,created_at,reason,step_key')");
    expect(src).toContain("flowEventsHistorySql('action,actor_name,created_at,reason,step_key', ' AND kind=? AND record_id=?')");
  });

  it('retirement:ts-statements is strictly lower than the recorded 53 r41 baseline after the r42/r44/r45/r46/r47 native read and projection cut', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(53);
    // r46 moved the approval-step/expense-batch reads onto native kinds (43->41);
    // r47 moved calendar-index-facts and record-details history onto native kinds (41->37);
    // r48 deduped the approver union, flow version guard, and settlement sources (37->33);
    // r49 deduped the export-receipt result read onto the closed command-receipt projection (33->32).
    expect(total).toBe(32);
  });
});
