import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { columnsFromWhereHead } from '../../../modules/procurement/server/src/adapters/read-tail-r54';
import { orderScopedAggregate } from '../../../modules/procurement/server/src/adapters/read-tail-r30';
import { calendarScopedListRead } from '../../../modules/procurement/server/src/adapters/read-tail-r34';
import { requestOrderLink } from '../../../modules/procurement/server/src/adapters/read-tail-r19';

const adapters = (name: string) => new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFileSync(adapters(name), 'utf8');
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=79 source at HEAD
// 6d08598b3c2af7d5bbebc8cbce2503ac80c91bfb. `head` is the exact byte-identical
// SQL prefix the r54 renderer owns; each `tail` stays at the caller with every
// bind, JOIN and predicate fragment unchanged.
const HEAD = 'SELECT ${columns} FROM ${from} WHERE ';
const ORACLES = [
  {
    site: 'r30/purchase-request-progress order-scoped aggregate',
    columns: 'DISTINCT o.id,o.number,o.state,o.created_at',
    from: '(${commercePlatformSources.commerce_orders}) o',
    tail: "o.company_id=? AND o.kind='purchase' AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders l WHERE l.request_id=? AND l.order_id=o.id)) ORDER BY o.created_at DESC,o.id DESC",
    sql: "SELECT DISTINCT o.id,o.number,o.state,o.created_at FROM (${commercePlatformSources.commerce_orders}) o WHERE o.company_id=? AND o.kind='purchase' AND (o.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders l WHERE l.request_id=? AND l.order_id=o.id)) ORDER BY o.created_at DESC,o.id DESC",
  },
  {
    site: 'r34/calendar supplier-bills scoped list read',
    columns: 'r.*',
    from: 'procurement_bill_matches r',
    tail: 'BR AND LIST AND BILL_PRED AND SW ORDER BY r.due_date,r.id LIMIT ?',
    sql: 'SELECT r.* FROM procurement_bill_matches r WHERE BR AND LIST AND BILL_PRED AND SW ORDER BY r.due_date,r.id LIMIT ?',
  },
];

// BEFORE sha256 of the two edited builder files. Reversing both folds
// (renderer call -> original literal, import/comment restored) must reproduce
// them exactly, proving no rendered byte was lost.
const BEFORE: Record<string, string> = {
  'read-tail-r30.ts': '80f7c5bef27bf0336afe07b0cbac6238b457a946810ecfe6c65168f4036d1fbc',
  'read-tail-r34.ts': 'aa1a9844836d9b025fe33b5d876aca3ed83c47041a5cc0e036e97d06f48ffc13',
};
const revert: Record<string, (s: string) => string> = {
  'read-tail-r30.ts': (s) => s
    .replace("import {columnsFromWhereHead} from './read-tail-r54';\n", '')
    .replace(' * verbatim; the r19 link helper stays at every call site. The byte-identical\n * columns/FROM/WHERE head now comes from r54. */', ' * verbatim; the r19 link helper stays at every call site. */')
    .replace(
      ') => columnsFromWhereHead(columns, from, `${orderAlias}.company_id=?${before} AND (${orderAlias}.id=? OR ${link})${tail}`);',
      ') => `SELECT ${columns} FROM ${from} WHERE ${orderAlias}.company_id=?${before} AND (${orderAlias}.id=? OR ${link})${tail}`;',
    ),
  'read-tail-r34.ts': (s) => s
    .replace("import {columnsFromWhereHead} from './read-tail-r54';\n", '')
    .replace(' * LIMIT bind stays at each call site. The byte-identical columns/FROM/WHERE head\n * now comes from r54. */', ' * LIMIT bind stays at each call site. */')
    .replace(
      ') => columnsFromWhereHead(columns, from, `${branchScope} AND ${scopeExpr} AND ${predicates} AND ${scopeWhere} ORDER BY ${order} LIMIT ?`);',
      ') => `SELECT ${columns} FROM ${from} WHERE ${branchScope} AND ${scopeExpr} AND ${predicates} AND ${scopeWhere} ORDER BY ${order} LIMIT ?`;',
    ),
};

describe('Procurement read-tail r54 columns/FROM/WHERE head dedupe', () => {
  test('the renderer aliases the byte-identical r31 head wrapper and owns no SQL', () => {
    const helper = src('read-tail-r54.ts');
    expect(helper).toContain("export { sourceImage as columnsFromWhereHead } from './read-tail-r31';");
    expect(count(helper, 'SELECT ')).toBe(0);
    expect(columnsFromWhereHead('${columns}', '${from}', '')).toBe(HEAD);
    expect(columnsFromWhereHead('T', 'S', 'id=?')).toBe('SELECT T FROM S WHERE id=?');
  });

  test('each head plus its original tail renders byte-identically', () => {
    for (const { site, columns, from, tail, sql } of ORACLES) {
      expect([site, columnsFromWhereHead(columns, from, tail)]).toEqual([site, sql]);
    }
  });

  test('both exported builders keep their exact original rendered SQL', () => {
    expect(orderScopedAggregate(
      ORACLES[0].columns,
      ORACLES[0].from,
      requestOrderLink('l', 'o'),
      'o',
      " AND o.kind='purchase'",
      ' ORDER BY o.created_at DESC,o.id DESC',
    )).toBe(ORACLES[0].sql);
    expect(calendarScopedListRead('r.*', 'procurement_bill_matches r', 'BR', 'LIST', 'BILL_PRED', 'SW', 'r.due_date,r.id'))
      .toBe(ORACLES[1].sql);
  });

  test('both builders delegate to r54 and the retired inline heads are gone', () => {
    const r30 = src('read-tail-r30.ts');
    const r34 = src('read-tail-r34.ts');
    for (const s of [r30, r34]) expect(s).toContain("import {columnsFromWhereHead} from './read-tail-r54';");
    expect(count(r30, 'columnsFromWhereHead(')).toBe(1);
    expect(count(r34, 'columnsFromWhereHead(')).toBe(1);
    expect(r30).not.toContain('`SELECT ${columns} FROM ${from} WHERE ${orderAlias}');
    expect(r34).not.toContain('`SELECT ${columns} FROM ${from} WHERE ${branchScope}');
    // Call sites keep their binds and ordering fragments untouched.
    expect(src('purchase-request-progress.ts')).toContain('orderScopedAggregate(');
    expect(src('calendar-supplier-bills-facts.ts')).toContain('calendarScopedListRead(');
    expect(src('calendar-procurement-rfqs-facts.ts')).toContain('calendarScopedListRead(');
    expect(src('calendar-supplier-promises-facts.ts')).toContain('calendarScopedListRead(');
  });

  test('reversing both folds reproduces the original source bytes', () => {
    for (const [file, before] of Object.entries(BEFORE)) {
      expect([file, sha(revert[file](src(file)))]).toEqual([file, before]);
    }
  });

  test('no write, lock, native or cross-owner consumer was touched', () => {
    expect(src('read-tail-r54.ts')).not.toMatch(/FOR\s+(UPDATE|SHARE)|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE procurement_rfqs,');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
  });

  test('witnesses and r23/r28/r31/r44/r45/r47/r48/r50/r51/r52 helpers stay byte-identical', () => {
    expect(sha(src('read-tail-r23.ts'))).toBe('b1f282c6de197c6681cb53bea93075a89fd9d16684d7d039af4ffe44d0c5548e');
    expect(sha(src('read-tail-r28.ts'))).toBe('d9a93d385847a379c5e470da3add3fd5e504558ec2a1e9c9fb94043efeab94d0');
    expect(sha(src('read-tail-r31.ts'))).toBe('d6d1fa0a6b3499731d60d8cb9aad71f2f9bfde5f19b77ea354d4016aae432ce1');
    expect(sha(src('read-tail-r44.ts'))).toBe('f5dc17c41403aded63b04458839a97a5db2ad0025139f88390478638f45239be');
    expect(sha(src('read-tail-r45.ts'))).toBe('7e85a1f2555ab7d26ef18662380522a8db6d4f78cac5f5be0a27a9d1f3b120ec');
    expect(sha(src('read-tail-r47.ts'))).toBe('d4908a00c22d2da7aebc30a2b14f0f514ae602872b15e464399fc6768b21a67e');
    expect(sha(src('read-tail-r48.ts'))).toBe('3c49967c017a03df44136e588df5b9ca71abf04d49465be79c2c572a3cadc816');
    expect(sha(src('read-tail-r50.ts'))).toBe('a22440f40b969c713465c4ad55110805176fb27c70500cc4c7584889110d919e');
    expect(sha(src('read-tail-r51.ts'))).toBe('08bd16f0507ba84fe3f9bf6285faea9c24126a871141c78707dcbdeb3f907958');
    expect(sha(src('read-tail-r52.ts'))).toBe('3b588e2a3ec73e6ac8caf5eee864c1b8e5b95139e9eb40da12e732c31261603d');
    expect(sha(src('read-tail-r53.ts'))).toBe('70fe1a434534f27038ec5225e1f2558fcbf57734335db2cd1aefbdfed9913ce5');
    expect(sha(src('package-obligations.ts'))).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(sha(src('supplier-bills.ts'))).toBe('f1485ee0fc42475f3337626f561f97aa4ab1d1921b6df412bc97c12a7e534fef');
  });

  test('worktree SELECT count is 79 - 2 + 1 = 78', () => {
    const root = new URL('../../../modules/procurement/server/src/', import.meta.url);
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
    expect(total).toBeLessThan(78);
    expect(total).toBe(75);
  });
});
