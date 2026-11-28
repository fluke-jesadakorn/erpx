import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { nativeApprovalPlanningPorts } from '../../../modules/workflow/server/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../modules/workflow/server/src');
const instancePrefix = "EXISTS(SELECT 1 FROM approval_instances WHERE id=? AND version=? AND state='";

// Guards are built synchronously; no DB execution occurs.
const db = { batch: () => { throw new Error('unexpected batch'); } } as unknown as SqlDatabase;
const guards = (nativeApprovalPlanningPorts(db).repository as unknown as {
  guard16: (...args: unknown[]) => { kind: string; values: unknown[] };
  guard36: (...args: unknown[]) => { kind: string; values: unknown[] };
});

describe('approval-step negative-existence prefix dedupe (r36)', () => {
  it('guard16 emits byte-identical blocked-state SQL and bind order', () => {
    const stmt = guards.guard16('key', 'legacy-route', 'id', 1, 'id2');
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values).toEqual([
      'key', 'legacy-route', 'id', 1, 'id2',
      instancePrefix + "blocked') AND NOT EXISTS(SELECT 1 FROM approval_steps WHERE instance_id=?)",
    ]);
  });

  it('guard36 emits byte-identical override-independent SQL and bind order', () => {
    const stmt = guards.guard36('key', 'override-independent', 'u1', 'r1', 'c1', null, 'u2', null, 'c2', 'id', 3, 'u3', 'c3');
    expect(stmt.kind).toBe('wf_native_command_assertions_3');
    expect(stmt.values).toEqual([
      'key', 'override-independent', 'u1', 'r1', 'c1', null, 'u2', null, 'c2', 'id', 3, 'u3', 'c3',
      "NOT iam_same_person(?, ?, ?) AND (?::text IS NULL OR NOT iam_same_person(?, ?, ?)) AND NOT EXISTS(SELECT 1 FROM approval_steps s WHERE s.instance_id=? AND s.ordinal<? AND s.state='approved' AND iam_same_person(s.approver_user_id,?,?))",
    ]);
  });

  it('keeps one shared prefix head in the native contract and no inline duplicates in callers', async () => {
    const [caller, contract] = await Promise.all([
      readFile(new URL('../../../modules/workflow/server/src/adapters/approval-planning.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/workflow/server/src/workflow-read-fragments-sql.json', import.meta.url), 'utf8'),
    ]);
    const head = /NOT EXISTS\(SELECT 1 FROM approval_steps/g;
    expect(caller.match(head) ?? []).toHaveLength(0);
    expect(contract.match(head) ?? []).toHaveLength(1);
    expect(caller).not.toContain("blocked') AND NOT EXISTS(SELECT 1 FROM approval_steps WHERE instance_id=?)");
    expect(caller).not.toContain("AND NOT EXISTS(SELECT 1 FROM approval_steps s WHERE");
    expect(caller).toContain('fragments.noApprovalStepExists');
  });

  it('retirement:ts-statements is strictly lower than the recorded 57 r35 baseline', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(57);
    expect(total).toBeLessThanOrEqual(55);
  });
});
