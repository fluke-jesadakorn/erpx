import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeApprovalPlanningPorts } from '../../../modules/workflow/server/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
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

  it('keeps the byte-identical shared prefix head in the native contract, not the caller', async () => {
    const [caller, contract] = await Promise.all([
      readFile(new URL('../../../modules/workflow/server/src/adapters/approval-planning.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/workflow/server/src/workflow-read-fragments-sql.json', import.meta.url), 'utf8'),
    ]);
    const head = /EXISTS\(SELECT 1 FROM approval_instances WHERE id=\? AND version=\? AND state='/g;
    expect(caller.match(head) ?? []).toHaveLength(0);
    expect(contract.match(/"approvalInstanceStatePrefix":/g) ?? []).toHaveLength(1);
    expect(JSON.parse(contract).approvalInstanceStatePrefix).toMatch(head);
    expect(caller).toContain('fragments.approvalInstanceStatePrefix');
  });
});
