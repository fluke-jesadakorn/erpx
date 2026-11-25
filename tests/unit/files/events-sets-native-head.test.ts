import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_EVENTS_WHERE_HEAD_SQL,
  ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL,
} from '../../../packages/server-files/src/adapters/read-tail-r45-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'packages/server-files/src');
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** Byte-exact oracles captured from the two retired TypeScript heads. */
const HEADS = [
  [ATTACHMENT_EVENTS_WHERE_HEAD_SQL, '7ad49cdd02f1f6e41d2813b289318c83775d66a1face9b0876a708d1f99e0981', 38, 'SELECT 1 FROM attachment_events WHERE '],
  [ATTACHMENT_SETS_STAR_WHERE_HEAD_SQL, 'faf86a30a11aeb95423273b1dd3d78a6ecb6d32b94c6c05ccdf3867c5087dccd', 36, 'SELECT * FROM attachment_sets WHERE '],
] as const;

describe('Files event/set heads are native-owned', () => {
  test('both exports render byte-exactly from the Rust-owned contract', () => {
    for (const [sql, hash, len, expected] of HEADS) {
      expect(sha(sql)).toBe(hash);
      expect(Buffer.byteLength(sql, 'utf8')).toBe(len);
      expect(sql).toBe(expected);
      expect((sql.match(/SELECT /g) ?? []).length).toBe(1);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  test('no statement token remains in the TypeScript helper', () => {
    const helper = src('adapters/read-tail-r45-source-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(helper).not.toContain(token);
    expect(helper).toContain('events-sets-head-sources.json');
    expect(helper).not.toContain('prepare(');
    expect(helper).not.toContain('.bind(');
  });

  test('the head bytes live in the Rust contract and native port', () => {
    const json = JSON.parse(src('adapters/events-sets-head-sources.json'));
    expect(Object.keys(json)).toEqual(['attachmentEventsWhere', 'attachmentSetsStarWhere']);
    expect((Object.values(json) as string[]).map(sha).sort()).toEqual(HEADS.map(([, hash]) => hash).sort());
    const rust = src('adapters/events_sets_head.rs');
    expect(rust).toContain('include_str!("events-sets-head-sources.json")');
    for (const fn of ['attachment_events_where', 'attachment_sets_star_where']) {
      expect(rust).toContain(`pub fn ${fn}()`);
      expect(src('public/events_sets_head.rs')).toContain(fn);
    }
    expect(src('public/events_sets_head.rs')).toContain('pub use crate::adapters::events_sets_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod events_sets_head;');
    expect(src('public/mod.rs')).toContain('pub mod events_sets_head;');
  });

  test('the service-repository call site still reads both exports', () => {
    expect(src('adapters/attachments/service-repository.ts')).toContain('read-tail-r45-source-sql');
  });

  test('retirement:ts-statements is 190, strictly below the Field-owner BEFORE=192 baseline', () => {
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
    expect(literal).toBeLessThan(192);
    expect(total).toBeLessThan(193);
  });
});
