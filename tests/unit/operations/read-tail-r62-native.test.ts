import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  OPERATIONS_RD_ATTACHMENT_OWNER_SCOPE,
  nativeOperationsReadFirst,
} from '../../../modules/operations/server/src/adapters/native-operations-read';

const srcRoot = resolve(__dirname, '../../../modules/operations/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/operations/server/src/adapters/${name}`, import.meta.url), 'utf8');
const registry = () => src('native_operations_read_sql.rs');
const facade = () => readFile(new URL('../../../modules/operations/server/src/public/native_operations_read.rs', import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

/** Pre-r62 byte oracle: the exact retired inline staff_task scope literal. */
const retired = 'SELECT company_id,branch_id,department_id,version FROM staff_tasks WHERE id=?';
const oracle = 'b481dd311f73819cb25fff6c9eb7534f3a97b5a894664a04722af4a315bb2672';

describe('operations read-tail r62 attachment owner-scope native cutover', () => {
  it('pins the TS read-kind id, Rust registry entry and public facade re-export', async () => {
    expect(OPERATIONS_RD_ATTACHMENT_OWNER_SCOPE).toBe('operations_rd_attachment_owner_scope');
    const [native, surface] = await Promise.all([registry(), facade()]);
    expect(native).toContain('"operations_rd_attachment_owner_scope"');
    expect(native).toContain('pub const RD_ATTACHMENT_OWNER_SCOPE: &str');
    expect(native).toContain('READ_ATTACHMENT_OWNER_SCOPE,');
    expect(surface).toContain('RD_ATTACHMENT_OWNER_SCOPE');
  });

  it('grows the native allow-list from 24 to 25 kinds with no duplicate entry', async () => {
    const native = await registry();
    expect(native.match(/const READ_[A-Z_]+: NativeReadStatement/g) ?? []).toHaveLength(25);
    expect(native.match(/RD_ATTACHMENT_OWNER_SCOPE/g) ?? []).toHaveLength(5);
  });

  it('keeps the retired SELECT literal byte-identical in Rust and absent from TS', async () => {
    expect(retired.match(/SELECT /g)).toHaveLength(1);
    expect(retired.match(/\?/g)).toHaveLength(1);
    expect(sha(retired)).toBe(oracle);
    const [native, adapter] = await Promise.all([registry(), src('attachment-migration-owner.ts')]);
    expect(native).toContain(retired);
    expect(native.split('pub const RD_ATTACHMENT_OWNER_SCOPE:').at(1)?.split('};')[0]).toContain('bind_count: 1');
    expect(adapter).not.toContain(retired);
    expect(adapter).not.toMatch(/SELECT\s/);
    expect(adapter).not.toContain('.prepare(');
    expect(adapter).toContain('nativeOperationsReadFirst');
    expect(adapter).toContain("if (kind !== 'staff_task') return null;");
  });

  it('sends exactly the pinned kind and values to the owner plan and runs on the caller unit', async () => {
    const previous = process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
    process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3999', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' }],
    });
    const values = ['task-id'];
    try {
      const prepare = vi.fn(() => ({ bind: vi.fn() }));
      const batch = vi.fn(async () => [{
        results: [{ company_id: 'company', branch_id: 'branch', department_id: 'dept', version: 3 }],
        success: true, meta: { changes: 0, duration: 0 },
      }]);
      const db = { prepare, batch } as unknown as SqlDatabase;
      const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
        new Response(JSON.stringify({ data: { statements: [{ sql: retired, values }] } }), { status: 200, headers: { 'content-type': 'application/json' } }),
      );
      try {
        const row = await nativeOperationsReadFirst(OPERATIONS_RD_ATTACHMENT_OWNER_SCOPE, values, db);
        expect(row).toEqual({ company_id: 'company', branch_id: 'branch', department_id: 'dept', version: 3 });
        const call = fetchMock.mock.calls[0]!;
        expect(String(call[0])).toContain('/operations/command-plan');
        expect(JSON.parse(String(call[1]!.body))).toMatchObject({ kind: 'operations_rd_attachment_owner_scope', values });
        expect(prepare).toHaveBeenCalledWith(retired);
        expect(batch).toHaveBeenCalledTimes(1);
      } finally {
        fetchMock.mockRestore();
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
      await expect(nativeOperationsReadFirst(OPERATIONS_RD_ATTACHMENT_OWNER_SCOPE, ['task-id'], db))
        .rejects.toMatchObject({ status: 503, code: 'OPERATIONS_NATIVE_READ_UNAVAILABLE' });
      expect(db.prepare).not.toHaveBeenCalled();
      expect(db.batch).not.toHaveBeenCalled();
    } finally {
      if (previous !== undefined) process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = previous;
    }
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 61 to 60', async () => {
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
    expect(total).toBeLessThan(61);
  });
});
