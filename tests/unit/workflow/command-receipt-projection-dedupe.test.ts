import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { commandReceiptByCommandSql } from '../../../modules/workflow/server/src/adapters/receipt-sql';
import { departmentOverrideReceiptOperation } from '../../../modules/workflow/server/src/adapters/department-override-facts';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const pointReadSql = (column: string) => `SELECT ${column} FROM command_receipts WHERE command_id=?`;

function capture() {
  const calls: { sql: string; binds: unknown[] }[] = [];
  const stmt = (sql: string) => ({
    bind(...values: unknown[]) { calls.push({ sql, binds: values }); return this; },
    first() { return undefined; },
    all() { return { results: [] as unknown[] }; },
  });
  const db = { prepare: (sql: string) => stmt(sql) } as unknown as SqlDatabase;
  return { db, calls };
}

describe('workflow command-receipt projection generator family (r31)', () => {
  it('emits byte-identical SQL for both projected columns', () => {
    expect(commandReceiptByCommandSql('record_id')).toBe('SELECT record_id FROM command_receipts WHERE command_id=?');
    expect(commandReceiptByCommandSql('operation')).toBe('SELECT operation FROM command_receipts WHERE command_id=?');
    expect(commandReceiptByCommandSql('operation')).toBe(pointReadSql('operation'));
  });
  it('departmentOverrideReceiptOperation prepares the operation projection with unchanged binds', async () => {
    const { db, calls } = capture();
    expect(await departmentOverrideReceiptOperation(db, 'key-1')).toBeNull();
    expect(calls).toEqual([{ sql: 'SELECT operation FROM command_receipts WHERE command_id=?', binds: ['key-1'] }]);
  });
  it('keeps one generator definition and no inline point-read literals', async () => {
    const { readFile } = await import('node:fs/promises');
    const [shared, facts, projections] = await Promise.all([
      readFile(new URL('../../../modules/workflow/server/src/adapters/receipt-sql.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/workflow/server/src/adapters/department-override-facts.ts', import.meta.url), 'utf8'),
      readFile(new URL('../../../modules/workflow/server/src/adapters/closed-projection-sources.ts', import.meta.url), 'utf8'),
    ]);
    expect(projections.match(/`\$\{selectHeadSql\}\$\{columns\.join\(separator\)\} FROM command_receipts\$\{tail\}`/g) ?? []).toHaveLength(1);
    expect(shared).toContain("commandReceiptsProjectionSql([column], ',', ' WHERE command_id=?')");
    expect(shared).not.toContain('`SELECT ${column} FROM command_receipts');
    expect(shared).toContain("commandReceiptByCommandSql('record_id')");
    expect(facts).not.toContain('SELECT operation FROM command_receipts');
    expect(facts).toContain("commandReceiptByCommandSql('operation')");
  });
});
