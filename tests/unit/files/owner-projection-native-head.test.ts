import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_LINKS_OWNER_PROJECTION_SQL,
  ATTACHMENT_SETS_OWNER_PROJECTION_SQL,
  ATTACHMENTS_OWNER_PROJECTION_SQL,
} from '../../../packages/server-files/src/adapters/owner-projection-dedupe-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** Byte-exact oracles captured from the three retired TypeScript literals. */
const HEADS = [
  [ATTACHMENTS_OWNER_PROJECTION_SQL, 'f6866e2eeec1ac46c5b12da98afcd528cda55808f7fbabfb51c4bbb0e6fbf7d6', 415, 'attachments'],
  [ATTACHMENT_LINKS_OWNER_PROJECTION_SQL, '083488efc2f6fb5b7000edef251890fba67770fefd6e8819f3b2623b1a4171dd', 244, 'attachment_links'],
  [ATTACHMENT_SETS_OWNER_PROJECTION_SQL, '3b9871ec69b9e9746557a4b7c7b2a595e8f1af83f9a1427d12774e43f79582b4', 412, 'attachment_sets'],
] as const;

describe('Files owner projections are native-owned', () => {
  test('the three exports render byte-exactly from the Rust-owned contract', () => {
    for (const [sql, hash, len, table] of HEADS) {
      expect(sha(sql)).toBe(hash);
      expect(Buffer.byteLength(sql, 'utf8')).toBe(len);
      expect((sql.match(/SELECT /g) ?? []).length).toBe(1);
      expect(sql.startsWith('SELECT "id",')).toBe(true);
      expect(sql.endsWith(` FROM ${table}`)).toBe(true);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  test('no statement token remains in the TypeScript helper', () => {
    const helper = src('adapters/owner-projection-dedupe-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(helper).not.toContain(token);
    expect(helper).toContain('owner-projection-head-sources.json');
    expect(helper).toContain('export const ATTACHMENTS_OWNER_PROJECTION_SQL');
    expect(helper).toContain('export const ATTACHMENT_LINKS_OWNER_PROJECTION_SQL');
    expect(helper).toContain('export const ATTACHMENT_SETS_OWNER_PROJECTION_SQL');
    expect(helper).not.toContain('prepare(');
    expect(helper).not.toContain('.bind(');
  });

  test('the statement bytes live in the Rust contract and native port', () => {
    const json = JSON.parse(src('adapters/owner-projection-head-sources.json'));
    expect(Object.keys(json)).toEqual(['attachments', 'attachmentLinks', 'attachmentSets']);
    const hashes = (Object.values(json) as string[]).map(sha).sort();
    expect(hashes).toEqual(HEADS.map(([, hash]) => hash).sort());
    const rust = src('adapters/owner_projection_head.rs');
    expect(rust).toContain('include_str!("owner-projection-head-sources.json")');
    for (const fn of ['attachments_projection', 'attachment_links_projection', 'attachment_sets_projection']) {
      expect(rust).toContain(`pub fn ${fn}()`);
      expect(src('public/owner_projection_head.rs')).toContain(fn);
    }
    expect(src('public/owner_projection_head.rs')).toContain('pub use crate::adapters::owner_projection_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod owner_projection_head;');
    expect(src('public/mod.rs')).toContain('pub mod owner_projection_head;');
  });

  test('the projection consumers still read the same three exports', () => {
    for (const rel of [
      'adapters/final-root-owner-projection-sources.ts',
      'adapters/hospitality-owner-projection-sources.ts',
      'adapters/commerce-final-owner-projection-sources.ts',
      'adapters/last-audit-owner-projection-sources.ts',
    ]) {
      expect(src(rel)).toContain('from "./owner-projection-dedupe-sql"');
    }
    expect(src('adapters/finance-owned-snapshot-sources.ts')).toContain('from "./owner-projection-dedupe-sql"');
  });

  test('retirement:ts-statements is 207, strictly below the owner-projection BEFORE=210 baseline', () => {
    let literal = 0, total = 0, broad = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
      broad += (text.match(/\b(SELECT|INSERT|UPDATE|DELETE)\b/g) ?? []).length;
    }
    expect(literal).toBe(207);
    expect(total).toBe(208);
    expect(broad).toBe(226);
    expect(literal).toBeLessThan(210);
    expect(total).toBeLessThan(211);
  });
});
