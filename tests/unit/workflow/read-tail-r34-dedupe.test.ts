import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeApprovalPlanningPorts } from '../../../packages/server-workflow/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { readFile } from 'node:fs/promises';

// Guard descriptors are built synchronously; no DB execution occurs.
const db = { batch: () => { throw new Error('unexpected batch'); } } as unknown as SqlDatabase;
const guards = (nativeApprovalPlanningPorts(db).repository as unknown as {
  guard16: (...args: unknown[]) => { kind: string; values: unknown[] };
  guard31: (...args: unknown[]) => { kind: string; values: unknown[] };
});

const prefix = "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='";
const pending = prefix + "pending' AND current_step=?)";
const blocked = prefix + "blocked') AND NOT EXISTS(SELECT 1 FROM approval_steps WHERE instance_id=?)";

describe('instance-state guard prefix dedupe (r34)', () => {
  it('guard16 emits byte-identical blocked-state SQL and bind order', () => {
    const stmt = guards.guard16('key', 'legacy-route', 'id', 1, 'id2');
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values).toEqual(['key', 'legacy-route', 'id', 1, 'id2', blocked]);
  });

  it('guard31 still emits unchanged pending-state SQL (r25 span preserved)', () => {
    const stmt = guards.guard31('key', 'approval-version', 'id', 1, 0, 'id2', 'user', true);
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values).toEqual([
      'key', 'approval-version', 'id', 1, 0, 'id2', 'user',
      pending + ' AND EXISTS(SELECT 1 FROM approval_steps WHERE id=? AND state=\'pending\' AND approver_user_id<>?)',
    ]);
  });

  it('keeps one byte-identical shared prefix literal in source', async () => {
    const src = await readFile(new URL('../../../packages/server-workflow/src/adapters/approval-planning.ts', import.meta.url), 'utf8');
    expect(src.match(/EXISTS\(SELECT 1 FROM approval_instances WHERE id=\? AND version=\? AND state='/g)).toHaveLength(1);
  });
});
