import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_STAR_ID_HEAD_SQL,
  SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL,
  UPLOAD_INTENTS_EXISTS_HEAD_SQL,
} from '../../../modules/files/server/src/adapters/read-tail-r44-source-sql';

const rootDir = resolve(__dirname, '../../..');
const srcRoot = resolve(rootDir, 'modules/files/server/src');
const src = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

const tsFiles = (dir: string): string[] => readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
  const path = resolve(dir, entry.name);
  return entry.isDirectory() ? tsFiles(path) : entry.name.endsWith('.ts') ? [path] : [];
});

/** Byte-exact oracles captured from the three retired TypeScript heads. */
const HEADS = [
  [UPLOAD_INTENTS_EXISTS_HEAD_SQL, '560ec6b0b217da22318a4017afecf1d406a24d831c88963b6e21d506dfae79f2', 42, 'EXISTS(SELECT 1 FROM upload_intents WHERE '],
  [ATTACHMENT_STAR_ID_HEAD_SQL, '7c94985ae3e467cbac01d6528e74a58109ec8da120560b9852d647daf24d567f', 36, 'SELECT * FROM attachments WHERE id=?'],
  [SCAN_ATTEMPTS_OR_EXISTS_HEAD_SQL, '98857b768050e0d3446d1957739be4aca508a30073355a49ace70d3fb0901a86', 49, 'OR EXISTS(SELECT 1 FROM attachment_scan_attempts '],
] as const;

describe('Files upload/bind witness heads are native-owned', () => {
  test('the three exports render byte-exactly from the Rust-owned contract', () => {
    for (const [sql, hash, len, expected] of HEADS) {
      expect(sha(sql)).toBe(hash);
      expect(Buffer.byteLength(sql, 'utf8')).toBe(len);
      expect(sql).toBe(expected);
      expect((sql.match(/SELECT /g) ?? []).length).toBe(1);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  test('no statement token remains in the TypeScript helper', () => {
    const helper = src('adapters/read-tail-r44-source-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(helper).not.toContain(token);
    expect(helper).toContain('witness-head-sources.json');
    expect(helper).not.toContain('prepare(');
    expect(helper).not.toContain('.bind(');
  });

  test('the head bytes live in the Rust contract and native port', () => {
    const json = JSON.parse(src('adapters/witness-head-sources.json'));
    expect(Object.keys(json)).toEqual(['uploadIntentsExists', 'attachmentStarId', 'scanAttemptsOrExists']);
    expect((Object.values(json) as string[]).map(sha).sort()).toEqual(HEADS.map(([, hash]) => hash).sort());
    const rust = src('adapters/witness_head.rs');
    expect(rust).toContain('include_str!("witness-head-sources.json")');
    for (const fn of ['upload_intents_exists', 'attachment_star_id', 'scan_attempts_or_exists']) {
      expect(rust).toContain(`pub fn ${fn}()`);
      expect(src('public/witness_head.rs')).toContain(fn);
    }
    expect(src('public/witness_head.rs')).toContain('pub use crate::adapters::witness_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod witness_head;');
    expect(src('public/mod.rs')).toContain('pub mod witness_head;');
  });

  test('the witness call sites still read the same three exports', () => {
    for (const rel of [
      'adapters/attachments/service-repository.ts',
      'adapters/attachments/service.ts',
      'adapters/collaboration-home-sources.ts',
      'adapters/document-derivatives.ts',
      'adapters/messaging-file-settlements.ts',
      'adapters/messaging-upload-rollback.ts',
      'adapters/portal-package-obligation-sources.ts',
    ]) {
      expect(src(rel)).toContain('read-tail-r44-source-sql');
    }
  });

  test('retirement:ts-statements is 190, strictly below the projection BEFORE=201 baseline', () => {
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
    expect(literal).toBeLessThan(201);
    expect(total).toBeLessThan(202);
  });
});
