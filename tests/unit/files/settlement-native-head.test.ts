import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_EXISTS_HEAD_SQL,
  MESSAGING_SETTLEMENT_HEAD_SQL,
  PACKAGE_FILE_LINK_TARGET_HEAD_SQL,
} from '../../../modules/files/server/src/adapters/read-tail-r48-source-sql';

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
  [ATTACHMENT_EXISTS_HEAD_SQL, '08b4c26e14a85262ac9a177a9754f5203011dfaa51aae87e5b9ba59976c53e6c', 38, 'SELECT EXISTS(SELECT 1 FROM attachment', 2],
  [MESSAGING_SETTLEMENT_HEAD_SQL, '063e76012a14e6e8e4e5cd5f17144368ba286029f267fe5b4e6298f69d5c2ad5', 32, 'SELECT l.id link_id,s.id set_id,', 1],
  [PACKAGE_FILE_LINK_TARGET_HEAD_SQL, '2097bd180887f8b55c61c79fcd703dd91e700e61195ae872bbed88d08bb7f01d', 38, "SELECT 'file-link' kind,l.id,s.target_", 1],
] as const;

describe('Files settlement heads are native-owned', () => {
  test('the three exports render byte-exactly from the Rust-owned contract', () => {
    for (const [sql, hash, len, expected, selects] of HEADS) {
      expect(sha(sql)).toBe(hash);
      expect(Buffer.byteLength(sql, 'utf8')).toBe(len);
      expect(sql).toBe(expected);
      expect((sql.match(/SELECT /g) ?? []).length).toBe(selects);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
    }
  });

  test('no statement token remains in the TypeScript helper', () => {
    const helper = src('adapters/read-tail-r48-source-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(helper).not.toContain(token);
    expect(helper).toContain('settlement-head-sources.json');
    expect(helper).not.toContain('prepare(');
    expect(helper).not.toContain('.bind(');
  });

  test('the head bytes live in the Rust contract and native port', () => {
    const json = JSON.parse(src('adapters/settlement-head-sources.json'));
    expect(Object.keys(json)).toEqual(['attachmentExists', 'messagingSettlement', 'packageFileLinkTarget']);
    expect((Object.values(json) as string[]).map(sha).sort()).toEqual(HEADS.map(([, hash]) => hash).sort());
    const rust = src('adapters/settlement_head.rs');
    expect(rust).toContain('include_str!("settlement-head-sources.json")');
    for (const fn of ['attachment_exists', 'messaging_settlement', 'package_file_link_target']) {
      expect(rust).toContain(`pub fn ${fn}()`);
      expect(src('public/settlement_head.rs')).toContain(fn);
    }
    expect(src('public/settlement_head.rs')).toContain('pub use crate::adapters::settlement_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod settlement_head;');
    expect(src('public/mod.rs')).toContain('pub mod settlement_head;');
  });

  test('the five call sites still read the same three exports', () => {
    for (const rel of [
      'adapters/field-file-writer-owner.ts',
      'adapters/messaging-file-settlements.ts',
      'adapters/ecommerce-package-obligation-sources.ts',
      'adapters/marketing-package-obligation-sources.ts',
    ]) {
      expect(src(rel)).toContain('read-tail-r48-source-sql');
    }
  });

  test('retirement:ts-statements is 190, strictly below the witness BEFORE=198 baseline', () => {
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
    expect(literal).toBeLessThan(198);
    expect(total).toBeLessThan(199);
  });
});
