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
