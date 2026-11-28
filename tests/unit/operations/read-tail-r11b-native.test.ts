import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { workTaskSocialFileSourceRows } from '@erpx/collaboration-server/public/work-task-purchase-source';
import {
  OPERATIONS_TASK_DELEGATION_SOURCE_SQL,
  OPERATIONS_TASK_TEMPLATE_SOURCE_SQL,
  OPERATIONS_TASK_SCHEDULE_SOURCE_SQL,
} from '../../../modules/operations/server/src/adapters/task-delegation-source';
import {
  OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS,
  OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS_OWNED,
  nativeOperationsReadFirst,
} from '../../../modules/operations/server/src/adapters/native-operations-read';

const srcRoot = resolve(__dirname, '../../../modules/operations/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/operations/server/src/adapters/${name}`, import.meta.url), 'utf8');
const registry = () => src('native_operations_read_sql.rs');
const facade = () => readFile(new URL('../../../modules/operations/server/src/public/native_operations_read.rs', import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const ownerLiteral = (native: string, kind: string) => {
  const match = native.match(new RegExp(`const SQL_${kind}: &str =\\s*("(?:[^"\\\\]|\\\\.)*");`));
  if (!match) throw new Error(`missing SQL literal for ${kind}`);
  return JSON.parse(match[1]!);
};

/** Live recomposition of the retired TS witness literal. Keeps the byte oracle
 *  honest if any cross-owner fragment drifts after the cut. */
const compose = (ownedAction: boolean) => `SELECT t.id,t.company_id,t.branch_id,t.department_id,t.project_id,t.state,t.creator_user_id,t.assignee_user_id,t.template_id,social.work_social,
 jsonb_build_object('task',to_jsonb(t)-'version'-'updated_at'${ownedAction ? "-'state'-'completion_note'-'blocked_reason'" : ''},
 'template',${OPERATIONS_TASK_TEMPLATE_SOURCE_SQL},
 'schedule',${OPERATIONS_TASK_SCHEDULE_SOURCE_SQL},
 'delegations',${OPERATIONS_TASK_DELEGATION_SOURCE_SQL},
 'social',social.image)::text witness
 FROM staff_tasks t LEFT JOIN (${workTaskSocialFileSourceRows}) social ON social.id=t.id AND social.company_id=t.company_id WHERE t.id=? AND t.company_id=?`;

const variants = [
  {
    name: 'ordinary',
    kind: 'TASK_ATTACHMENT_PURCHASE_WITNESS',
    id: 'operations_rd_task_attachment_purchase_witness',
    owned: false,
    oracle: 'd7b8e6f3b6a54fde937a7394c0fb759c6fc6fb811fa34d918ee3db5b3bb6949c',
  },
  {
    name: 'owned-action',
    kind: 'TASK_ATTACHMENT_PURCHASE_WITNESS_OWNED',
    id: 'operations_rd_task_attachment_purchase_witness_owned',
    owned: true,
    oracle: 'f107faf99b1793e099d2107bc317551cd7365f839009c6e4d25f674475799261',
  },
] as const;

describe('operations read-tail r11b attachment purchase witness native cutover', () => {
  it('pins both TS read-kind ids, Rust registry entries and public facade re-exports', async () => {
    const [native, surface] = await Promise.all([registry(), facade()]);
    for (const v of variants) {
      expect(v.id).toBe(`operations_rd_${v.kind.toLowerCase()}`);
      expect(native).toContain(`"${v.id}"`);
      expect(native).toContain(`pub const RD_${v.kind}: &str`);
      expect(native).toContain(`READ_${v.kind},`);
      expect(surface).toContain(`RD_${v.kind}`);
    }
  });

  it('grows the native allow-list from 23 to 25 kinds with no duplicate entry', async () => {
    const native = await registry();
    expect(native.match(/const READ_[A-Z_]+: NativeReadStatement/g) ?? []).toHaveLength(25);
    for (const v of variants) {
      expect(native.match(new RegExp(`\\bRD_${v.kind}\\b`, 'g')) ?? []).toHaveLength(4);
    }
  });

  it('keeps both composed SELECT literals byte-identical in Rust and absent from TS', async () => {
    const [native, adapter] = await Promise.all([registry(), src('task-attachment-purchases.ts')]);
    for (const v of variants) {
      const live = compose(v.owned);
      expect(live.match(/SELECT /g)).toHaveLength(30);
      expect(live.match(/\?/g)).toHaveLength(2);
      expect(sha(live)).toBe(v.oracle);
      expect(ownerLiteral(native, v.kind)).toBe(live);
      expect(native.split(`pub const RD_${v.kind}:`).at(1)?.split('};')[0]).toContain('bind_count: 2');
    }
    expect(adapter).not.toMatch(/SELECT /);
    expect(adapter).not.toContain('workTaskSocialFileSourceRows');
    expect(adapter).not.toContain('OPERATIONS_TASK_TEMPLATE_SOURCE_SQL');
    expect(adapter).toContain('nativeOperationsReadFirst');
    expect(adapter).toContain('OPERATIONS_RD_TASK_ATTACHMENT_PURCHASE_WITNESS_OWNED');
  });

  it('sends exactly the pinned kind and values to the owner plan and runs on the caller unit', async () => {
    const previous = process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
    process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3999', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' }],
    });
    const values = ['task-id', 'company'];
    try {
      const native = await registry();
      for (const v of variants) {
        const sql = ownerLiteral(native, v.kind);
        const prepare = vi.fn(() => ({ bind: vi.fn() }));
        const batch = vi.fn(async () => [{ results: [{ id: 'a', witness: 'w' }], success: true, meta: { changes: 0, duration: 0 } }]);
        const db = { prepare, batch } as unknown as SqlDatabase;
        const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
          new Response(JSON.stringify({ data: { statements: [{ sql, values }] } }), { status: 200, headers: { 'content-type': 'application/json' } }),
        );
        try {
          const row = await nativeOperationsReadFirst(v.id, values, db);
          expect(row).toEqual({ id: 'a', witness: 'w' });
          const call = fetchMock.mock.calls[0]!;
          expect(String(call[0])).toContain('/operations/command-plan');
          expect(JSON.parse(String(call[1]!.body))).toMatchObject({ kind: v.id, values });
          expect(prepare).toHaveBeenCalledWith(sql);
          expect(batch).toHaveBeenCalledTimes(1);
        } finally {
          fetchMock.mockRestore();
        }
      }
    } finally {
      if (previous === undefined) delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
      else process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = previous;
    }
  });

  it('fails closed when the native read service is unavailable, with no TypeScript fallback', async () => {
    const previous = process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
    delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
    try {
      const db = { prepare: vi.fn(), batch: vi.fn() } as unknown as SqlDatabase;
      for (const v of variants) {
        await expect(nativeOperationsReadFirst(v.id, ['task-id', 'company'], db))
          .rejects.toMatchObject({ status: 503, code: 'OPERATIONS_NATIVE_READ_UNAVAILABLE' });
      }
      expect(db.prepare).not.toHaveBeenCalled();
      expect(db.batch).not.toHaveBeenCalled();
    } finally {
      if (previous !== undefined) process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = previous;
    }
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 59 to 58', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(58);
    expect(total).toBeLessThan(59);
  });
});
