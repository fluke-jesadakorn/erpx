import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ORPHAN_PRESERVATION_PENDING_SQL,
  ORPHAN_PRESERVATION_PRESENT_SQL,
} from '../../../packages/server-files/src/adapters/preservation-presence-dedupe-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** Byte-exact oracles captured from the two retired TypeScript literals. */
const HEADS = [
  [ORPHAN_PRESERVATION_PRESENT_SQL, '9022c17669f8f302c5338cff0b0ced7b493c81e7e77de316e1fced2f6f7b8a9e', 61, 1],
  [ORPHAN_PRESERVATION_PENDING_SQL, '6b250eb0f749824ae50868d942a53e34b8dfbcc6a1271551b9510d965ecead73', 195, 2],
] as const;

describe('Files orphan-preservation witnesses are native-owned', () => {
  test('both exports render byte-exactly from the Rust-owned contract', () => {
    for (const [sql, hash, len, selects] of HEADS) {
      expect(sha(sql)).toBe(hash);
      expect(Buffer.byteLength(sql, 'utf8')).toBe(len);
      expect((sql.match(/SELECT /g) ?? []).length).toBe(selects);
      expect(sql.startsWith('SELECT ')).toBe(true);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  test('no statement token remains in the TypeScript helper', () => {
    const helper = src('adapters/preservation-presence-dedupe-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(helper).not.toContain(token);
    expect(helper).toContain('orphan-preservation-head-sources.json');
    expect(helper).toContain('export const ORPHAN_PRESERVATION_PRESENT_SQL');
    expect(helper).toContain('export const ORPHAN_PRESERVATION_PENDING_SQL');
    expect(helper).not.toContain('prepare(');
    expect(helper).not.toContain('.bind(');
  });

  test('the statement bytes live in the Rust contract and native port', () => {
    const json = JSON.parse(src('adapters/orphan-preservation-head-sources.json'));
    expect(Object.keys(json)).toEqual(['presence', 'pending']);
    expect((Object.values(json) as string[]).map(sha)).toEqual(HEADS.map(([, hash]) => hash));
    const rust = src('adapters/orphan_preservation_head.rs');
    expect(rust).toContain('include_str!("orphan-preservation-head-sources.json")');
    for (const fn of ['preservation_present', 'preservation_pending']) {
      expect(rust).toContain(`pub fn ${fn}()`);
      expect(src('public/orphan_preservation_head.rs')).toContain(fn);
    }
    expect(src('public/orphan_preservation_head.rs')).toContain('pub use crate::adapters::orphan_preservation_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod orphan_preservation_head;');
    expect(src('public/mod.rs')).toContain('pub mod orphan_preservation_head;');
  });

  test('the preservation consumers still read the same two exports', () => {
    for (const rel of [
      'adapters/runtime-preservation-sources.ts',
      'adapters/runtime-minio-preservation-sources.ts',
      'adapters/attachments/migration.ts',
    ]) {
      expect(src(rel)).toContain('preservation-presence-dedupe-sql');
    }
  });

  test('retirement:ts-statements is 190, strictly below the owner-projection BEFORE=207 baseline', () => {
    let literal = 0, total = 0, broad = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
      broad += (text.match(/\b(SELECT|INSERT|UPDATE|DELETE)\b/g) ?? []).length;
    }
    expect(literal).toBe(190);
    expect(total).toBe(191);
    expect(broad).toBe(208);
    expect(literal).toBeLessThan(207);
    expect(total).toBeLessThan(208);
  });
});
