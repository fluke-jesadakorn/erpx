import { readFile, readdir, access } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../modules/workflow/server/src');
const src = (rel: string) => readFile(resolve(srcRoot, rel), 'utf8');

/** Byte oracle captured from the retired TypeScript literal before the r39 cut. */
const ORACLE = 'SELECT a.id,a.action,a.created_at,COALESCE(p.legal_name,a.actor_id) actor_name FROM audit_events a LEFT JOIN hr_people p ON p.user_id=a.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE a.record_id=? ORDER BY a.created_at DESC';

const readAdapter = (file: string) =>
  readFile(resolve(srcRoot, `adapters/${file}`), 'utf8');

describe('workflow inventory audit-history read tail (r39 native read cutover)', () => {
  it('deletes the TS SQL source and leaves no owner SQL in either switched adapter', async () => {
    await expect(access(resolve(srcRoot, 'adapters/inventory-audit-history-source.ts'))).rejects.toThrow();
    const [count, transfers] = await Promise.all([
      readAdapter('inventory-counts-sources.ts'),
      readAdapter('inventory-transfers-sources.ts'),
    ]);
    for (const source of [count, transfers]) {
      expect(source).not.toContain('SELECT ');
      expect(source).not.toContain('.prepare(');
      expect(source).toContain("from './native-workflow-read'");
      expect(source.match(/nativeWorkflowReadAll<.*>\('wf_native_inventory_audit_history'/g) ?? []).toHaveLength(1);
    }
  });

  it('keeps the retired literal byte-identical in the owner Rust read registry', async () => {
    const rust = await src('domain/workflow_wave3.rs');
    expect(rust).toContain(`"${ORACLE}"`);
    expect(rust).toContain('pub const WF_NATIVE_INVENTORY_AUDIT_HISTORY: &str = "wf_native_inventory_audit_history";');
    expect((ORACLE.match(/\?/g) ?? []).length).toBe(2);
  });

  it('plans countHistory through the native read with the exact company and record binds', async () => {
    const all = vi.fn(async () => [{ id: 'a1' }]);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowWrite: vi.fn(),
    }));
    try {
      const { inventoryCountSources } = await import(
        '../../../modules/workflow/server/src/adapters/inventory-counts-sources'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const rows = await inventoryCountSources(db).countHistory('company', 'record-1');
      expect(rows).toEqual({ results: [{ id: 'a1' }] });
      expect(all).toHaveBeenCalledWith(
        'wf_native_inventory_audit_history',
        ['company', 'record-1'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });

  it('plans transferHistory through the native read with the exact company and record binds', async () => {
    const all = vi.fn(async () => [{ id: 't1' }]);
    vi.resetModules();
    vi.doMock('../../../modules/workflow/server/src/adapters/native-workflow-read', () => ({
      nativeWorkflowReadAll: all,
      nativeWorkflowWrite: vi.fn(),
    }));
    try {
      const { inventoryTransferSources } = await import(
        '../../../modules/workflow/server/src/adapters/inventory-transfers-sources'
      );
      const prepare = vi.fn();
      const db = { prepare } as never;
      const rows = await inventoryTransferSources(db).transferHistory('company', 'record-2');
      expect(rows).toEqual({ results: [{ id: 't1' }] });
      expect(all).toHaveBeenCalledWith(
        'wf_native_inventory_audit_history',
        ['company', 'record-2'],
        'company',
        db,
      );
      expect(prepare).not.toHaveBeenCalled();
    } finally {
      vi.doUnmock('../../../modules/workflow/server/src/adapters/native-workflow-read');
      vi.resetModules();
    }
  });

  it('retirement:ts-statements is 20 after the r54 guard-head fold (23->20)', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT/g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(20);
  });
});
