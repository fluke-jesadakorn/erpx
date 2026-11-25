import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeApprovalPlanningPorts } from '../../../packages/server-workflow/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

// guard31 builds a native assertion descriptor synchronously; no DB execution occurs.
const db = { batch: () => { throw new Error('unexpected batch'); } } as unknown as SqlDatabase;
const ports = nativeApprovalPlanningPorts(db);
const guard31 = (ports.repository as unknown as {
  guard31: (...args: unknown[]) => { kind: string; values: unknown[] };
}).guard31;

const pending = "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='pending' AND current_step=?)";
const stepPrefix = pending + " AND EXISTS(SELECT 1 FROM approval_steps WHERE id=? AND ";

function guardSql(override: boolean): string {
  const stmt = guard31('key', 'approval-version', 'id', 1, 0, 'id2', 'user', override);
  expect(stmt.kind).toBe('wf_native_command_assertions_3');
  expect(stmt.values.slice(0, 7)).toEqual(['key', 'approval-version', 'id', 1, 0, 'id2', 'user']);
  return stmt.values[stmt.values.length - 1] as string;
}

describe('approval-version guard dedupe (r27)', () => {
  it('emits byte-identical SQL for the override branch', () => {
    expect(guardSql(true)).toBe(stepPrefix + "state='pending' AND approver_user_id<>?)");
  });
  it('emits byte-identical SQL for the non-override branch', () => {
    expect(guardSql(false)).toBe(
      stepPrefix + "? IN (SELECT approver_user_id UNION SELECT value FROM erpx_json_each(COALESCE(approver_pool_json,'[]'))) AND state='pending')",
    );
  });
});

// guard17 (legacy-record) and guard49 (route-version) share the byte-identical
// record/version EXISTS prefix; no DB execution occurs when building descriptors.
const guard17 = (ports.repository as unknown as {
  guard17: (...args: unknown[]) => { kind: string; values: unknown[] };
}).guard17;
const guard49 = (ports.repository as unknown as {
  guard49: (...args: unknown[]) => { kind: string; values: unknown[] };
}).guard49;

const recordExists = (versionFirst: boolean) =>
  `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND ${versionFirst ? 'version=? AND status=?' : 'status=? AND version=?'})`;

describe('approval record/version guard dedupe (r28)', () => {
  it('guard17 emits the unchanged byte-identical SQL and bind order', () => {
    const stmt = guard17('key', 'legacy-record', 'rec', 'co', 'pending_approval', 3, 'expense');
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values).toEqual(['key', 'legacy-record', 'rec', 'co', 'pending_approval', 3, recordExists(false)]);
  });
  it('guard49 emits the unchanged byte-identical SQL and bind order', () => {
    const stmt = guard49('key', 'route-version', 'inst', 5, 2, 'rec', 'co', 9, 'pending', 'expense');
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values).toEqual(['key', 'route-version', 'inst', 5, 2, 'rec', 'co', 9, 'pending', pending + ' AND ' + recordExists(true)]);
  });
  it('shares one byte-identical EXISTS head literal (r50 folds it into closed-projection-sources)', async () => {
    const { readFile } = await import('node:fs/promises');
    const [src, shared] = await Promise.all([
      readFile(new URL('../../../packages/server-workflow/src/adapters/approval-planning.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../packages/server-workflow/src/adapters/closed-projection-sources.ts', import.meta.url), 'utf8'),
    ]);
    expect(src.match(/EXISTS\(SELECT 1 FROM \$\{table\} WHERE /g) ?? []).toHaveLength(0);
    expect(shared.match(/EXISTS\(SELECT 1 FROM \$\{table\} WHERE /g) ?? []).toHaveLength(1);
    expect(src).toContain('tableExistsGuardSql(table, `id=? AND company_id=? AND ');
  });
});
