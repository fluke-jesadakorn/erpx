import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  ATTACHMENT_ID_AND_HEAD_SQL,
  FIELD_SET_UPLOAD_EXISTS_HEAD_SQL,
  FIELD_UPLOAD_ANCESTOR_EXISTS_SQL,
} from '../../../modules/files/server/src/adapters/read-tail-r43-source-sql';

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
  [FIELD_UPLOAD_ANCESTOR_EXISTS_SQL, '0ae8d0da4cbc376c4c67dcc009a0630bcea15d9c12531039c2b10d69b6cbe81f', 102, 'EXISTS(SELECT 1 FROM attachments a WHERE a.upload_id=u.id AND erpx_files_field_attachment_owned(a.id))', 1],
  [FIELD_SET_UPLOAD_EXISTS_HEAD_SQL, 'b6fb6dc590d9a2654782fc3f69f76848a491da0f314ce27e29aa2e9d608a534e', 76, 'OR EXISTS(SELECT 1 FROM upload_intents u WHERE u.attachment_set_id=s.id AND ', 1],
  [ATTACHMENT_ID_AND_HEAD_SQL, '3d18048b4a0975e642531d845a0301c5a531574430a9231de600435e8bc3f220', 36, 'FROM attachments a WHERE a.id=? AND ', 0],
] as const;

describe('Files Field-owner heads are native-owned', () => {
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
    const helper = src('adapters/read-tail-r43-source-sql.ts');
    for (const token of ['SELECT', 'INSERT', 'DELETE', 'UPDATE ']) expect(helper).not.toContain(token);
    expect(helper).toContain('field-owner-head-sources.json');
    expect(helper).not.toContain('prepare(');
    expect(helper).not.toContain('.bind(');
  });

  test('the head bytes live in the Rust contract and native port', () => {
    const json = JSON.parse(src('adapters/field-owner-head-sources.json'));
    expect(Object.keys(json)).toEqual(['fieldUploadAncestor', 'fieldSetUpload', 'attachmentIdAnd']);
    expect((Object.values(json) as string[]).map(sha).sort()).toEqual(HEADS.map(([, hash]) => hash).sort());
    const rust = src('adapters/field_owner_head.rs');
    expect(rust).toContain('include_str!("field-owner-head-sources.json")');
    for (const fn of ['field_upload_ancestor', 'field_set_upload', 'attachment_id_and']) {
      expect(rust).toContain(`pub fn ${fn}()`);
      expect(src('public/field_owner_head.rs')).toContain(fn);
    }
    expect(src('public/field_owner_head.rs')).toContain('pub use crate::adapters::field_owner_head::');
    expect(src('adapters/mod.rs')).toContain('pub mod field_owner_head;');
    expect(src('public/mod.rs')).toContain('pub mod field_owner_head;');
  });

  test('the Field and commerce call sites still read the same three exports', () => {
    for (const rel of ['adapters/field-file-writer-owner.ts', 'adapters/commerce-facts.ts']) {
      expect(src(rel)).toContain('read-tail-r43-source-sql');
    }
  });

  test('retirement:ts-statements is 190, strictly below the settlement BEFORE=194 baseline', () => {
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
    expect(literal).toBeLessThan(194);
    expect(total).toBeLessThan(195);
  });
});
