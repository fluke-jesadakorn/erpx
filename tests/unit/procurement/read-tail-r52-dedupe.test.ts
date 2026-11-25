import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { byIdExistsHead } from '../../../packages/server-procurement/src/adapters/read-tail-r52';
import { sourceVisible } from '../../../packages/server-procurement/src/adapters/read-tail-r25';
import { versionStatusScopeGuard } from '../../../packages/server-procurement/src/adapters/read-tail-r30';

const adapters = (name: string) => new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFileSync(adapters(name), 'utf8');
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=81 source at HEAD
// f78a1fffa6b7414b1e6258d78d0900e8c7929dd7. `head` is the exact byte-identical
// SQL prefix the r52 renderer owns; each `tail` stays at the caller with every
// bind, JOIN and ordering fragment unchanged.
const HEAD = 'EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=';
const ORACLES = [
  {
    site: 'r25/sourceVisible rfq',
    table: 'procurement_rfqs', alias: 'procurement_source',
    tail: '${id} AND procurement_source.company_id=${company} AND ${rfq.where})',
    sql: 'EXISTS(SELECT 1 FROM procurement_rfqs procurement_source WHERE procurement_source.id=${id} AND procurement_source.company_id=${company} AND ${rfq.where})',
  },
  {
    site: 'r25/sourceVisible bill',
    table: 'procurement_bill_matches', alias: 'procurement_bill_source',
    tail: '${id} AND procurement_bill_source.company_id=${company} AND ${bill.where})',
    sql: 'EXISTS(SELECT 1 FROM procurement_bill_matches procurement_bill_source WHERE procurement_bill_source.id=${id} AND procurement_bill_source.company_id=${company} AND ${bill.where})',
  },
  {
    site: 'r30/versionStatusScopeGuard rfq',
    table: 'procurement_rfqs', alias: 'r',
    tail: '? AND r.version=? AND r.status=? AND SCOPE)',
    sql: 'EXISTS(SELECT 1 FROM procurement_rfqs r WHERE r.id=? AND r.version=? AND r.status=? AND SCOPE)',
  },
  {
    site: 'r30/versionStatusScopeGuard bill',
    table: 'procurement_bill_matches', alias: 'm',
    tail: '? AND m.version=? AND m.status=? AND SCOPE)',
    sql: 'EXISTS(SELECT 1 FROM procurement_bill_matches m WHERE m.id=? AND m.version=? AND m.status=? AND SCOPE)',
  },
];

// BEFORE sha256 of the two edited builder files. Reversing both folds
// (renderer call -> original literal, import/comment restored) must reproduce
// them exactly, proving no rendered byte was lost.
const BEFORE: Record<string, string> = {
  'read-tail-r25.ts': '9eeec994b9f25684c26cda260dab2a887f76a1f6cd6fbe5b57b6e495ce23fee1',
  'read-tail-r30.ts': '9d440490659b1243b262e79d67d1844e404e2dbe5bba94edf68a16a1b0f76cc9',
};
const revert: Record<string, (s: string) => string> = {
  'read-tail-r25.ts': (s) => s
    .replace("import {byIdExistsHead} from './read-tail-r52';\n", '')
    .replace(
      "/** Visible actual source guarded by the owner's company/scope predicate. The\n * byte-identical SQL head now comes from r52; every tail and bind stays here. */",
      "/** Visible actual source guarded by the owner's company/scope predicate. */",
    )
    .replace(
      '`${byIdExistsHead(table, alias)}${id} AND ${alias}.company_id=${company} AND ${where})`',
      '`EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=${id} AND ${alias}.company_id=${company} AND ${where})`',
    ),
  'read-tail-r30.ts': (s) => s
    // r54 fold is reversed first, then the r52 fold, to reach the BEFORE bytes.
    .replace("import {columnsFromWhereHead} from './read-tail-r54';\n", '')
    .replace(' * verbatim; the r19 link helper stays at every call site. The byte-identical\n * columns/FROM/WHERE head now comes from r54. */', ' * verbatim; the r19 link helper stays at every call site. */')
    .replace(
      ') => columnsFromWhereHead(columns, from, `${orderAlias}.company_id=?${before} AND (${orderAlias}.id=? OR ${link})${tail}`);',
      ') => `SELECT ${columns} FROM ${from} WHERE ${orderAlias}.company_id=?${before} AND (${orderAlias}.id=? OR ${link})${tail}`;',
    )
    .replace("import {byIdExistsHead} from './read-tail-r52';\n", '')
    .replace(
      ' * command fences. The table, alias and scope fragment render verbatim; the\n * byte-identical SQL head now comes from r52. */',
      ' * command fences. The table, alias and scope fragment render verbatim. */',
    )
    .replace(
      '`${byIdExistsHead(table, alias)}? AND ${alias}.version=? AND ${alias}.status=? AND ${scope})`',
      '`EXISTS(SELECT 1 FROM ${table} ${alias} WHERE ${alias}.id=? AND ${alias}.version=? AND ${alias}.status=? AND ${scope})`',
    ),
};

describe('Procurement read-tail r52 by-id EXISTS head dedupe', () => {
  test('the renderer owns exactly one SELECT and emits the byte-identical head', () => {
    const helper = src('read-tail-r52.ts');
    expect(count(helper, 'export const byIdExistsHead =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(byIdExistsHead('${table}', '${alias}')).toBe(HEAD);
    expect(byIdExistsHead('T', 'a')).toBe('EXISTS(SELECT 1 FROM T a WHERE a.id=');
  });

  test('each head plus its original tail renders byte-identically', () => {
    for (const { site, table, alias, tail, sql } of ORACLES) {
      expect([site, byIdExistsHead(table, alias) + tail]).toEqual([site, sql]);
    }
  });

  test('both exported builders keep their exact original rendered SQL', () => {
    expect(sourceVisible('procurement_rfqs', 'procurement_source', '${id}', '${company}', '${rfq.where}')).toBe(ORACLES[0].sql);
    expect(sourceVisible('procurement_bill_matches', 'procurement_bill_source', '${id}', '${company}', '${bill.where}')).toBe(ORACLES[1].sql);
    expect(versionStatusScopeGuard('procurement_rfqs', 'r', 'SCOPE')).toBe(ORACLES[2].sql);
    expect(versionStatusScopeGuard('procurement_bill_matches', 'm', 'SCOPE')).toBe(ORACLES[3].sql);
  });

  test('both builders delegate to r52 and the retired inline heads are gone', () => {
    const r25 = src('read-tail-r25.ts');
    const r30 = src('read-tail-r30.ts');
    for (const s of [r25, r30]) expect(s).toContain("import {byIdExistsHead} from './read-tail-r52';");
    expect(count(r25, 'byIdExistsHead(')).toBe(1);
    expect(count(r30, 'byIdExistsHead(')).toBe(1);
    expect(r25).not.toContain('`EXISTS(SELECT 1 FROM ${table}');
    expect(r30).not.toContain('`EXISTS(SELECT 1 FROM ${table}');
    // Call sites keep their binds and ordering fragments untouched.
    expect(src('record-purchases.ts')).toContain('sourceVisible("procurement_rfqs","procurement_source",id,company,rfq.where)');
    expect(src('record-purchases.ts')).toContain('sourceVisible("procurement_bill_matches","procurement_bill_source",id,company,bill.where)');
    expect(src('rfqs.ts')).toContain('versionStatusScopeGuard("procurement_rfqs","r",scope)');
    expect(src('supplier-bills.ts')).toContain('versionStatusScopeGuard("procurement_bill_matches","m",scope)');
  });

  test('reversing both folds reproduces the original source bytes', () => {
    for (const [file, before] of Object.entries(BEFORE)) {
      expect([file, sha(revert[file](src(file)))]).toEqual([file, before]);
    }
  });

  test('no write, lock, native or cross-owner consumer was touched', () => {
    expect(src('read-tail-r52.ts')).not.toMatch(/FOR\s+(UPDATE|SHARE)|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE procurement_rfqs,');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
  });

  test('witnesses and r23/r28/r31/r44/r45/r47/r48/r50/r51 helpers stay byte-identical', () => {
    expect(sha(src('read-tail-r23.ts'))).toBe('b1f282c6de197c6681cb53bea93075a89fd9d16684d7d039af4ffe44d0c5548e');
    expect(sha(src('read-tail-r28.ts'))).toBe('d9a93d385847a379c5e470da3add3fd5e504558ec2a1e9c9fb94043efeab94d0');
    expect(sha(src('read-tail-r31.ts'))).toBe('d6d1fa0a6b3499731d60d8cb9aad71f2f9bfde5f19b77ea354d4016aae432ce1');
    expect(sha(src('read-tail-r44.ts'))).toBe('f5dc17c41403aded63b04458839a97a5db2ad0025139f88390478638f45239be');
    expect(sha(src('read-tail-r45.ts'))).toBe('7e85a1f2555ab7d26ef18662380522a8db6d4f78cac5f5be0a27a9d1f3b120ec');
    expect(sha(src('read-tail-r47.ts'))).toBe('7973cb62b2004263bc84593b15b1d5e5ce43dff50b7beea695fab3c6af4d2e6d');
    expect(sha(src('read-tail-r48.ts'))).toBe('3c49967c017a03df44136e588df5b9ca71abf04d49465be79c2c572a3cadc816');
    expect(sha(src('read-tail-r50.ts'))).toBe('a22440f40b969c713465c4ad55110805176fb27c70500cc4c7584889110d919e');
    expect(sha(src('read-tail-r51.ts'))).toBe('08bd16f0507ba84fe3f9bf6285faea9c24126a871141c78707dcbdeb3f907958');
    expect(sha(src('package-obligations.ts'))).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(sha(src('supplier-bills.ts'))).toBe('f1485ee0fc42475f3337626f561f97aa4ab1d1921b6df412bc97c12a7e534fef');
  });

  test('worktree SELECT count is 81 - 2 + 1 = 80, then r53 80 - 2 + 1 = 79, then r54 79 - 2 + 1 = 78', () => {
    const root = new URL('../../../packages/server-procurement/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(81);
    expect(total).toBe(77); // r53: two EXISTS/FROM/WHERE heads (-2 +1 helper) 80 -> 79; r54: two columns/FROM/WHERE heads (-2 +1 helper) 79 -> 78; r55: two ARRAY heads (-2 +1 helper) 78 -> 77.
  });
});
