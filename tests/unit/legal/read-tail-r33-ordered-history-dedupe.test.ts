import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { legalOrderedHistorySql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r30-sources';
import { legalPointByIdSql } from '../../../modules/legal/server/src/adapters/legal-read-tail-r32-sources';

const srcRoot = resolve(__dirname, '../../../modules/legal/server/src');
const src = (name: string) => readFile(new URL(`../../../modules/legal/server/src/adapters/${name}`, import.meta.url), 'utf8');
const ordered = (table: string, key: string) => `SELECT * FROM ${table} WHERE ${key}=? ORDER BY created_at,id`;

const members: [string, string][] = [
  ['legal_executed_reviews', 'executed_copy_id'],
  ['legal_feature_history', 'item_id'],
];

describe('legal read-tail r33 ordered-history family folds into the single point-read head', () => {
  it('renders byte-identical SQL for every ordered-history member', () => {
    for (const [table, key] of members) {
      expect(legalOrderedHistorySql(table, key)).toBe(ordered(table, key));
      expect(legalPointByIdSql('*', table).replace('WHERE id=?', `WHERE ${key}=?`) + ' ORDER BY created_at,id').toBe(ordered(table, key));
    }
  });

  it('deletes the duplicate literal and keeps exactly one point-read head', async () => {
    const [r30, r32, r36] = await Promise.all([
      src('legal-read-tail-r30-sources.ts'),
      src('legal-read-tail-r32-sources.ts'),
      src('legal-read-tail-r36-sources.ts'),
    ]);
    expect(r30).not.toContain('SELECT');
    expect(r30).toContain("legalPointByIdSql('*', table).replace('WHERE id=?', `WHERE ${key}=?`) + ' ORDER BY created_at,id'");
    expect(r30.match(/export const legalOrderedHistorySql/g) ?? []).toHaveLength(1);
    expect(r32).toContain("legalPointByKeySql(columns, table, 'id')");
    expect(r36.match(/export const legalPointByKeySql/g) ?? []).toHaveLength(1);
    expect(r36.match(/SELECT /g) ?? []).toHaveLength(1);
    expect(r36).toContain("'document_id' | 'id' | 'item_id' | 'destruction_id' | 'envelope_id'");
    for (const text of [r30, r32, r36]) {
      expect(text).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
      expect(text).not.toMatch(/db\.prepare|sqlx|include_str!|fallback/i);
    }
  });

  it('keeps the cross-slice count pin strictly below the recorded BEFORE=37', async () => {
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(p);
        else if (entry.name.endsWith('.ts')) total += ((await readFile(p, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(srcRoot);
    expect(total).toBe(35);
  });
});
