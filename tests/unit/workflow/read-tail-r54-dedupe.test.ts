import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { fragments } from '../../../packages/server-workflow/src/adapters/read-fragments';
import { nativeApprovalPlanningPorts } from '../../../packages/server-workflow/src/adapters/approval-planning';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../packages/server-workflow/src');
const src = (rel: string) => readFile(resolve(srcRoot, rel), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');
type Oracle = readonly [digest: string, length: number, binds: number];
const check = (sql: string, [digest, length, binds]: Oracle) => {
  expect(sha(sql)).toBe(digest);
  expect(sql.length).toBe(length);
  expect((sql.match(/\?/g) ?? []).length).toBe(binds);
};

// Byte oracles captured at HEAD 2c31e013 before the r54 guard-head fold. These
// are the same digests pinned by the Rust owner contract test in
// domain/read_fragments.rs: only the head source moved, not a byte or a bind.
const heads: Record<string, Oracle> = {
  approvalInstanceStatePrefix: ['bd81d7afd58ef3e0724e31ae6b80aa63fb509332eb2e84b8a870a263737fa425', 76, 2],
  pendingInstanceAndStep: ['4ec2791843403261db2a55cb5b152f4ae22e6ffdd933bedf16178c24296a9903', 160, 4],
  noApprovalStepExists: ['229084d5029d20e74ab83d16ef5384883a2356b225c2abe78d4d6b648551baae', 39, 0],
};

const instancePrefix = fragments.approvalInstanceStatePrefix;
const stepPrefix = fragments.noApprovalStepExists;
const db = { batch: () => { throw new Error('unexpected batch'); } } as unknown as SqlDatabase;
const guards = (nativeApprovalPlanningPorts(db).repository as unknown as {
  guard16: (...args: unknown[]) => { kind: string; values: unknown[] };
  guard31: (...args: unknown[]) => { kind: string; values: unknown[] };
  guard36: (...args: unknown[]) => { kind: string; values: unknown[] };
});

describe('read-tail r54 approval guard-head fold', () => {
  it('keeps the three native heads byte-identical with unchanged bind positions', () => {
    for (const [key, oracle] of Object.entries(heads)) check(fragments[key]!, oracle);
    expect(`${instancePrefix}pending' AND current_step=?)`).toContain("state='pending' AND current_step=?)");
  });

  it('emits guard16/guard31/guard36 SQL and bind order byte-identically to HEAD', () => {
    const g16 = guards.guard16('key', 'legacy-route', 'id', 1, 'id2');
    expect(g16.kind).toBe('wf_native_command_assertions_3');
    expect(g16.values).toEqual([
      'key', 'legacy-route', 'id', 1, 'id2',
      `${instancePrefix}blocked') AND ${stepPrefix} WHERE instance_id=?)`,
    ]);

    const g31 = guards.guard31('key', 'approval-version', 'id', 1, 0, 'id2', 'user', true);
    expect(g31.kind).toBe('wf_native_command_assertions_3');
    expect(g31.values).toEqual([
      'key', 'approval-version', 'id', 1, 0, 'id2', 'user',
      `${fragments.pendingInstanceAndStep}state='pending' AND approver_user_id<>?)`,
    ]);

    const g36 = guards.guard36('key', 'override-independent', 'u1', 'r1', 'c1', null, 'u2', null, 'c2', 'id', 3, 'u3', 'c3');
    expect(g36.kind).toBe('wf_native_command_assertions_3');
    expect(g36.values).toEqual([
      'key', 'override-independent', 'u1', 'r1', 'c1', null, 'u2', null, 'c2', 'id', 3, 'u3', 'c3',
      `NOT iam_same_person(?, ?, ?) AND (?::text IS NULL OR NOT iam_same_person(?, ?, ?)) AND ${stepPrefix} s WHERE s.instance_id=? AND s.ordinal<? AND s.state='approved' AND iam_same_person(s.approver_user_id,?,?))`,
    ]);
  });

  it('leaves each head only in the native contract and delegates from the TS caller', async () => {
    const [planning, contract] = await Promise.all([
      src('adapters/approval-planning.ts'),
      src('workflow-read-fragments-sql.json'),
    ]);
    expect(planning.match(/EXISTS\(SELECT 1 FROM approval_instances WHERE id=\? AND version=\? AND state='/g) ?? []).toHaveLength(0);
    expect(planning.match(/NOT EXISTS\(SELECT 1 FROM approval_steps/g) ?? []).toHaveLength(0);
    for (const key of Object.keys(heads)) {
      expect(planning).toContain(`fragments.${key}`);
      expect(contract.match(new RegExp(`"${key}":`, 'g')) ?? []).toHaveLength(1);
    }
    expect(planning).toContain("pending' AND current_step=?)");
  });

  it('folds read-only heads only and drops the TS SELECT tail 23->20', async () => {
    for (const [key, oracle] of Object.entries(heads)) {
      const sql = fragments[key]!;
      expect(sql.startsWith('EXISTS(SELECT 1') || sql.startsWith('NOT EXISTS(SELECT 1')).toBe(true);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
      check(sql, oracle);
    }
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBeLessThan(23);
    expect(total).toBe(20);
  });
});
