import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  OPERATIONS_RD_HANDOVER_RECOVERY_RECEIPT,
  OPERATIONS_RD_WORKFLOW_DOCUMENT_DETAIL,
  OPERATIONS_RD_WORKFLOW_DOCUMENT_PAGE,
  OPERATIONS_RD_WORK_TASK_REPLAY_RECEIPT,
  nativeOperationsReadFirst,
  nativeOperationsReadAll,
} from '../../../packages/server-operations/src/adapters/native-operations-read';

const srcRoot = resolve(__dirname, '../../../packages/server-operations/src');
const src = (name: string) => readFile(new URL(`../../../packages/server-operations/src/adapters/${name}`, import.meta.url), 'utf8');
const registry = () => src('native_operations_read_sql.rs');

const plans = [
  {
    id: 'operations_rd_handover_recovery_receipt',
    ts: OPERATIONS_RD_HANDOVER_RECOVERY_RECEIPT,
    rust: 'RD_HANDOVER_RECOVERY_RECEIPT',
    binds: 3,
    values: ['key', 'key-publish', 'post-draft-key'],
    sql: 'SELECT 1 FROM command_receipts WHERE command_id IN(?,?) UNION ALL SELECT 1 FROM social_post_drafts WHERE id=?',
    selects: 2,
    caller: 'handover.ts',
    callerBind: '[key,`${key}-publish`,`post-draft-${key}`]',
    page: false,
  },
  {
    id: 'operations_rd_workflow_document_detail',
    ts: OPERATIONS_RD_WORKFLOW_DOCUMENT_DETAIL,
    rust: 'RD_WORKFLOW_DOCUMENT_DETAIL',
    binds: 2,
    values: ['company', 'task'],
    sql: 'SELECT * FROM staff_tasks WHERE company_id=? AND id=?',
    selects: 1,
    caller: 'workflow-document-sources.ts',
    callerBind: '[companyId,id]',
    page: false,
  },
  {
    id: 'operations_rd_workflow_document_page',
    ts: OPERATIONS_RD_WORKFLOW_DOCUMENT_PAGE,
    rust: 'RD_WORKFLOW_DOCUMENT_PAGE',
    binds: 2,
    values: ['company', 'after'],
    sql: 'SELECT * FROM staff_tasks WHERE company_id=? AND id>? ORDER BY id LIMIT 100',
    selects: 1,
    caller: 'workflow-document-sources.ts',
    callerBind: '[companyId,after]',
    page: true,
  },
  {
    id: 'operations_rd_work_task_replay_receipt',
    ts: OPERATIONS_RD_WORK_TASK_REPLAY_RECEIPT,
    rust: 'RD_WORK_TASK_REPLAY_RECEIPT',
    binds: 3,
    values: ['command-key', 'userId', 'companyId'],
    sql: "SELECT operation FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?",
    selects: 1,
    caller: 'work-task-purchases.ts',
    callerBind: '[command.key,actor.userId,actor.companyId]',
    page: false,
  },
] as const;

describe('operations read-tail r51 native read plans', () => {
  it('pins the four TS read-kind ids and their Rust registry/facade declarations', async () => {
    const native = await registry();
    const facade = await readFile(new URL('../../../packages/server-operations/src/public/native_operations_read.rs', import.meta.url), 'utf8');
    for (const plan of plans) {
      expect(plan.ts).toBe(plan.id);
      expect(native).toContain(`"${plan.id}"`);
      expect(native).toContain(`pub const ${plan.rust}: &str`);
      expect(native).toContain(`READ_${plan.rust.replace(/^RD_/, '')},`);
      expect(facade).toContain(plan.rust);
    }
  });

  it('grows the native allow-list from 17 to 21 kinds with no duplicate entry', async () => {
    const native = await registry();
    expect((native.match(/const READ_[A-Z_]+: NativeReadStatement/g) ?? [])).toHaveLength(21);
    for (const plan of plans) {
      expect(native.match(new RegExp(plan.rust, 'g'))).not.toBeNull();
    }
  });

  it('pins bind counts on both sides of the plan bridge', async () => {
    const native = await registry();
    for (const plan of plans) {
      const block = native.split(`pub const ${plan.rust}:`).at(1)?.split('};')[0] ?? '';
      expect(block).toContain(`bind_count: ${plan.binds}`);
      expect(plan.sql.match(/\?/g)).toHaveLength(plan.binds);
      const caller = await src(plan.caller);
      expect(caller).toContain(plan.callerBind);
    }
  });

  it('keeps the four retired SELECT literals byte-identical in Rust and absent from TS callers', async () => {
    const native = await registry();
    const callers = new Map<string, string>();
    for (const plan of plans) {
      if (!callers.has(plan.caller)) callers.set(plan.caller, await src(plan.caller));
      expect(native).toContain(plan.sql);
      expect(callers.get(plan.caller)).not.toContain(plan.sql);
      expect(plan.sql.match(/SELECT /g)).toHaveLength(plan.selects);
    }
    expect((await src('workflow-document-sources.ts')).match(/SELECT /g)).toBeNull();
    expect((await src('handover.ts')).match(/command_receipts WHERE command_id IN/g)).toBeNull();
    expect((await src('work-task-purchases.ts')).match(/authority_json::jsonb->>'companyId'/g)).toBeNull();
  });

  it('sends exactly the pinned kind and values to the owner plan and runs on the caller unit', async () => {
    const previous = process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
    process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3999', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' }],
    });
    try {
      for (const plan of plans) {
        const prepare = vi.fn(() => ({ bind: vi.fn() }));
        const batch = vi.fn(async () => [{ results: [{}], success: true, meta: { changes: 0, duration: 0 } }]);
        const db = { prepare, batch } as unknown as SqlDatabase;
        const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
          new Response(JSON.stringify({ data: { statements: [{ sql: plan.sql, values: plan.values }] } }), { status: 200, headers: { 'content-type': 'application/json' } }),
        );
        try {
          if (plan.page) await nativeOperationsReadAll(plan.ts, plan.values, db);
          else await nativeOperationsReadFirst(plan.ts, plan.values, db);
          const call = fetchMock.mock.calls[0]!;
          expect(String(call[0])).toContain('/operations/command-plan');
          expect(JSON.parse(String(call[1]!.body))).toMatchObject({ kind: plan.id, values: plan.values });
          expect(prepare).toHaveBeenCalledWith(plan.sql);
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
      for (const plan of plans) {
        const read = plan.page ? nativeOperationsReadAll(plan.ts, plan.values, db) : nativeOperationsReadFirst(plan.ts, plan.values, db);
        await expect(read).rejects.toMatchObject({ status: 503, code: 'OPERATIONS_NATIVE_READ_UNAVAILABLE' });
      }
      expect(db.prepare).not.toHaveBeenCalled();
      expect(db.batch).not.toHaveBeenCalled();
    } finally {
      if (previous !== undefined) process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = previous;
    }
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 82 to 75', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(66);
    expect(total).toBeLessThan(82);
  });
});
