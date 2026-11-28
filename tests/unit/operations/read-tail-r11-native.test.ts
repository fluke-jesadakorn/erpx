import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  OPERATIONS_RD_TASK_DETAIL_ATTACHMENTS,
  nativeOperationsReadAll,
} from '../../../modules/operations/server/src/adapters/native-operations-read';

const srcRoot = resolve(__dirname, '../../../modules/operations/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/operations/server/src/adapters/${name}`, import.meta.url), 'utf8');
const registry = () => src('native_operations_read_sql.rs');
const facade = () => readFile(new URL('../../../modules/operations/server/src/public/native_operations_read.rs', import.meta.url), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

/** Pre-r11 byte oracle: the exact TypeScript-rendered task-detail attachment
 *  literal, including the Files owner's closed attachments projection. */
const retired = 'SELECT id,original_name,bytes,media_type FROM (SELECT "id","upload_id","owner_type","owner_id","purpose","object_key","revision","original_name","media_type","bytes","checksum_sha256","created_by","legal_hold","created_at","revoked_at","company_id","detected_media_type","source_state","scanned_at","scanner_definition_at","preview_object_key","preview_media_type","preview_checksum_sha256","preview_bytes","destroyed_at","legacy_sanitized_only" FROM attachments) attachments WHERE owner_type=\'staff_task\' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at';
const oracle = '24444a73b1be8a008b9648ddb5641e4ae645b9164d272f0c96a8de31b53b6039';

describe('operations read-tail r11 task-detail attachments native cutover', () => {
  it('pins the TS read-kind id, Rust registry entry and public facade re-export', async () => {
    expect(OPERATIONS_RD_TASK_DETAIL_ATTACHMENTS).toBe('operations_rd_task_detail_attachments');
    const [native, surface] = await Promise.all([registry(), facade()]);
    expect(native).toContain('"operations_rd_task_detail_attachments"');
    expect(native).toContain('pub const RD_TASK_DETAIL_ATTACHMENTS: &str');
    expect(native).toContain('READ_TASK_DETAIL_ATTACHMENTS,');
    expect(surface).toContain('RD_TASK_DETAIL_ATTACHMENTS');
  });

  it('grows the native allow-list from 24 to 25 kinds with no duplicate entry', async () => {
    const native = await registry();
    expect(native.match(/const READ_[A-Z_]+: NativeReadStatement/g) ?? []).toHaveLength(25);
    expect(native.match(/RD_TASK_DETAIL_ATTACHMENTS/g) ?? []).toHaveLength(5);
  });

  it('keeps the retired SELECT literal byte-identical in Rust and absent from TS', async () => {
    expect(retired.match(/SELECT /g)).toHaveLength(2);
    expect(retired.match(/\?/g)).toHaveLength(1);
    expect(sha(retired)).toBe(oracle);
    const [native, adapter] = await Promise.all([registry(), src('staff-tasks.ts')]);
    const rustLiteral = native.match(/const SQL_TASK_DETAIL_ATTACHMENTS: &str =\s*("(?:[^"\\]|\\.)*");/)?.[1];
    expect(rustLiteral).toBeDefined();
    expect(JSON.parse(rustLiteral!)).toBe(retired);
    expect(native.split('pub const RD_TASK_DETAIL_ATTACHMENTS:').at(1)?.split('};')[0]).toContain('bind_count: 1');
    expect(adapter).not.toContain(retired);
    expect(adapter).not.toContain('original_name,bytes,media_type FROM');
    expect(adapter).not.toMatch(/filesProjectionSources/);
    expect(adapter).toContain('nativeOperationsReadAll');
    expect(adapter).toContain('OPERATIONS_RD_TASK_DETAIL_ATTACHMENTS');
  });

  it('sends exactly the pinned kind and values to the owner plan and runs on the caller unit', async () => {
    const previous = process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
    process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3999', timeoutMs: 1000,
      holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h1', companyId: 'company' }],
    });
    const values = ['task-id'];
    const rows = [
      { id: 'a', original_name: 'x', bytes: 1, media_type: 'image/png' },
      { id: 'b', original_name: 'y', bytes: 2, media_type: 'text/plain' },
    ];
    try {
      const prepare = vi.fn(() => ({ bind: vi.fn() }));
      const batch = vi.fn(async () => [{ results: rows, success: true, meta: { changes: 0, duration: 0 } }]);
      const db = { prepare, batch } as unknown as SqlDatabase;
      const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
        new Response(JSON.stringify({ data: { statements: [{ sql: retired, values }] } }), { status: 200, headers: { 'content-type': 'application/json' } }),
      );
      try {
        const result = await nativeOperationsReadAll(OPERATIONS_RD_TASK_DETAIL_ATTACHMENTS, values, db);
        expect(result.results).toEqual(rows);
        const call = fetchMock.mock.calls[0]!;
        expect(String(call[0])).toContain('/operations/command-plan');
        expect(JSON.parse(String(call[1]!.body))).toMatchObject({ kind: 'operations_rd_task_detail_attachments', values });
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
      await expect(nativeOperationsReadAll(OPERATIONS_RD_TASK_DETAIL_ATTACHMENTS, ['task-id'], db))
        .rejects.toMatchObject({ status: 503, code: 'OPERATIONS_NATIVE_READ_UNAVAILABLE' });
      expect(db.prepare).not.toHaveBeenCalled();
      expect(db.batch).not.toHaveBeenCalled();
    } finally {
      if (previous !== undefined) process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG = previous;
    }
  });

  it('retirement:ts-statements lowers the operations SELECT KPI from 60 to 59', async () => {
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
    expect(total).toBeLessThan(60);
  });
});
