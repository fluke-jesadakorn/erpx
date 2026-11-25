import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { DISTINCT_SET_ROW_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r50-source-sql';
import { EVENT_ID_PROJECTION_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r52-source-sql';
import { SOURCE_STATE_HEAD_SQL } from '../../../packages/server-files/src/adapters/read-tail-r53-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** Byte-exact oracles captured from the three retired TypeScript heads. */
const HEADS = [
  [DISTINCT_SET_ROW_HEAD_SQL, 'e34ef9843e0667106b7a9500ecf72e08ba0173b1449bb1b72ae61ef936fa04ff', 24, 'SELECT DISTINCT set_row.'],
  [EVENT_ID_PROJECTION_HEAD_SQL, '6f47869b2f9aeabe91dd699ec8dc50e1faa133bdd178e9623ce840dfc4bcfaf4', 16, 'SELECT event.id,'],
  [SOURCE_STATE_HEAD_SQL, '192eae6b027b37d4af0d94d61f87b92417780aa3f14ef2e9481834b8f2a7f5e4', 19, 'SELECT source_state'],
] as const;

describe('Files scalar projection heads are native-owned', () => {
  test('the three exports render byte-exactly from the Rust-owned contract', () => {
    for (const [sql, hash, len, expected] of HEADS) {
      expect(sha(sql)).toBe(hash);
      expect(Buffer.byteLength(sql, 'utf8')).toBe(len);
      expect(sql).toBe(expected);
      expect((sql.match(/SELECT /g) ?? []).length).toBe(1);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  test('no statement token remains in the three TypeScript helpers', () => {
    for (const helper of [
      'read-tail-r50-source-sql.ts',
      'read-tail-r52-source-sql.ts',
      'read-tail-r53-source-sql.ts',
    ]) {
      const text = src(`adapters/${helper}`);
      for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(text).not.toContain(token);
      expect(text).toContain('projection-head-sources.json');
      expect(text).not.toContain('prepare(');
      expect(text).not.toContain('.bind(');
    }
  });

  test('the head bytes live in the Rust contract and native port', () => {
    const json = JSON.parse(src('adapters/projection-head-sources.json'));
    expect(Object.keys(json)).toEqual(['distinctSetRow', 'eventId', 'sourceState']);
    expect((Object.values(json) as string[]).map(sha).sort()).toEqual(HEADS.map(([, hash]) => hash).sort());
    const rust = src('adapters/projection_head.rs');
    expect(rust).toContain('include_str!("projection-head-sources.json")');
    for (const fn of ['distinct_set_row', 'event_id', 'source_state']) {
      expect(rust).toContain(`pub fn ${fn}()`);
      expect(src('public/projection_head.rs')).toContain(fn);
    }
    expect(src('public/projection_head.rs')).toContain('pub use crate::adapters::projection_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod projection_head;');
    expect(src('public/mod.rs')).toContain('pub mod projection_head;');
  });

  test('the four call sites still read the same three exports', () => {
    for (const rel of [
      'adapters/leave-evidence-sources.ts',
      'adapters/attachment-denied-read-audits.ts',
      'adapters/customer-portal-files.ts',
      'adapters/attachments/service-repository.ts',
    ]) {
      const text = src(rel);
      expect(/DISTINCT_SET_ROW_HEAD_SQL|EVENT_ID_PROJECTION_HEAD_SQL|SOURCE_STATE_HEAD_SQL/.test(text)).toBe(true);
    }
  });

  test('retirement:ts-statements is 201, strictly below the preservation BEFORE=204 baseline', () => {
    let literal = 0, total = 0, broad = 0;
    for (const path of tsFiles(srcRoot)) {
      const text = readFileSync(path, 'utf8');
      literal += (text.match(/SELECT /g) ?? []).length;
      total += (text.match(/SELECT\s/g) ?? []).length;
      broad += (text.match(/\b(SELECT|INSERT|UPDATE|DELETE)\b/g) ?? []).length;
    }
    expect(literal).toBe(201);
    expect(total).toBe(202);
    expect(broad).toBe(220);
    expect(literal).toBeLessThan(204);
    expect(total).toBeLessThan(205);
  });
});
