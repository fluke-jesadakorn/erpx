import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { jsonbAggHead } from '../../../packages/server-procurement/src/adapters/read-tail-r51';
import { jsonbAggBy } from '../../../packages/server-procurement/src/adapters/read-tail-r21';
import { jsonbAggJoinBy } from '../../../packages/server-procurement/src/adapters/read-tail-r22';

const adapters = (name: string) => new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFileSync(adapters(name), 'utf8');
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=82 source at HEAD
// 614418b37bd2aa6fdd9ea437956b9992bdd2054b. `head` is the exact byte-identical
// SQL prefix the r51 renderer owns; each `tail` stays at the caller with every
// bind, JOIN and ordering fragment unchanged.
const HEAD = '(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${source} ${alias} ';
const ORACLES = [
  {
    site: 'r21/package-obligations binding-by-match',
    source: 'procurement_bill_match_receipts', alias: 'binding', order: 'binding.fulfillment_id',
    tail: 'WHERE binding.match_id=bill.id)',
    sql: '(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.fulfillment_id) FROM procurement_bill_match_receipts binding WHERE binding.match_id=bill.id)',
  },
  {
    site: 'r22/command-source offer-line join',
    source: 'procurement_rfq_offer_lines', alias: 'line', order: 'line.id',
    tail: 'JOIN procurement_rfq_offers offer ON offer.id=line.offer_id WHERE offer.rfq_id=r.id)',
    sql: '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_rfq_offer_lines line JOIN procurement_rfq_offers offer ON offer.id=line.offer_id WHERE offer.rfq_id=r.id)',
  },
];

// BEFORE sha256 of the two edited builder files. Reversing the two folds
// (renderer call -> original literal, import/comment restored) must reproduce
// them exactly, proving no rendered byte was lost.
const BEFORE: Record<string, string> = {
  'read-tail-r21.ts': 'e93cf29e9a83c4244100a2e92ffc039cec4083ac5d2330500ffe5920f88abc6d',
  'read-tail-r22.ts': '75c9a9848715e0f919a2ec2dcc5ca3c19abddcf81f560e975c26c95b3c5e56d1',
};
const revert: Record<string, (s: string) => string> = {
  'read-tail-r21.ts': (s) => s
    .replace("import {jsonbAggHead} from './read-tail-r51';\n", '')
    .replace(
      ' * unchanged. The r17/r18/r19/r20 helpers stay untouched; the byte-identical\n * SQL head now comes from r51. */',
      ' * unchanged. The r17/r18/r19/r20 helpers stay untouched. */',
    )
    .replace(
      '`${jsonbAggHead(source, alias, order)}WHERE ${alias}.${fk}=${ref})`',
      '`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${source} ${alias} WHERE ${alias}.${fk}=${ref})`',
    ),
  'read-tail-r22.ts': (s) => s
    .replace("import {jsonbAggHead} from './read-tail-r51';\n", '')
    .replace(
      ' * unchanged. The r17/r18/r19/r20/r21 helpers stay untouched; the byte-identical\n * SQL head now comes from r51. */',
      ' * unchanged. The r17/r18/r19/r20/r21 helpers stay untouched. */',
    )
    .replace(
      '`${jsonbAggHead(source, alias, order)}JOIN ${joinSource} ${joinAlias} ON ${joinOn} WHERE ${joinAlias}.${fk}=${ref})`',
      '`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${order}) FROM ${source} ${alias} JOIN ${joinSource} ${joinAlias} ON ${joinOn} WHERE ${joinAlias}.${fk}=${ref})`',
    ),
};

describe('Procurement read-tail r51 jsonb_agg head dedupe', () => {
  test('the renderer owns exactly one SELECT and emits the byte-identical head', () => {
    const helper = src('read-tail-r51.ts');
    expect(count(helper, 'export const jsonbAggHead =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(jsonbAggHead('${source}', '${alias}', '${order}')).toBe(HEAD);
    expect(jsonbAggHead('S', 'a', 'o')).toBe('(SELECT jsonb_agg(to_jsonb(a) ORDER BY o) FROM S a ');
  });

  test('each head plus its original tail renders byte-identically', () => {
    for (const { site, source, alias, order, tail, sql } of ORACLES) {
      expect([site, jsonbAggHead(source, alias, order) + tail]).toEqual([site, sql]);
    }
  });

  test('both exported builders keep their exact original rendered SQL', () => {
    expect(jsonbAggBy('procurement_bill_match_receipts', 'binding', 'match_id', 'bill.id', 'binding.fulfillment_id')).toBe(ORACLES[0].sql);
    expect(jsonbAggBy('procurement_request_orders', 'binding', 'rfq_id', 'r.id', 'binding.request_id,binding.order_id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.request_id,binding.order_id) FROM procurement_request_orders binding WHERE binding.rfq_id=r.id)',
    );
    expect(jsonbAggJoinBy('procurement_rfq_offer_lines', 'line', 'procurement_rfq_offers', 'offer', 'offer.id=line.offer_id', 'rfq_id', 'r.id', 'line.id')).toBe(ORACLES[1].sql);
    expect(jsonbAggJoinBy('procurement_delivery_promise_lines', 'line', 'procurement_delivery_promises', 'promise', 'promise.id=line.promise_id', 'order_id', 'r.id', 'line.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_delivery_promise_lines line JOIN procurement_delivery_promises promise ON promise.id=line.promise_id WHERE promise.order_id=r.id)',
    );
  });

  test('both builders delegate to r51 and the retired inline heads are gone', () => {
    const r21 = src('read-tail-r21.ts');
    const r22 = src('read-tail-r22.ts');
    for (const s of [r21, r22]) expect(s).toContain("import {jsonbAggHead} from './read-tail-r51';");
    expect(count(r21, 'jsonbAggHead(')).toBe(1);
    expect(count(r22, 'jsonbAggHead(')).toBe(1);
    expect(r21).not.toContain('`(SELECT jsonb_agg(to_jsonb(');
    expect(r22).not.toContain('`(SELECT jsonb_agg(to_jsonb(');
    // Call sites keep their binds and ordering fragments untouched.
    expect(src('package-obligations.ts')).toContain('jsonbAggBy(');
    expect(src('attachment-read-sources.ts')).toContain('jsonbAggBy(');
    expect(src('command-source.ts')).toContain('jsonbAggJoinBy(');
  });

  test('reversing both folds reproduces the original source bytes', () => {
    for (const [file, before] of Object.entries(BEFORE)) {
      expect([file, sha(revert[file](src(file)))]).toEqual([file, before]);
    }
  });

  test('no write, lock or cross-owner consumer was touched', () => {
    expect(src('read-tail-r51.ts')).not.toMatch(/FOR\s+(UPDATE|SHARE)|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE procurement_rfqs,');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
  });

  test('witnesses and r28/r31/r44/r45/r47/r48/r50 helpers stay byte-identical', () => {
    expect(sha(src('read-tail-r28.ts'))).toBe('d9a93d385847a379c5e470da3add3fd5e504558ec2a1e9c9fb94043efeab94d0');
    expect(sha(src('read-tail-r31.ts'))).toBe('d6d1fa0a6b3499731d60d8cb9aad71f2f9bfde5f19b77ea354d4016aae432ce1');
    expect(sha(src('read-tail-r44.ts'))).toBe('f5dc17c41403aded63b04458839a97a5db2ad0025139f88390478638f45239be');
    expect(sha(src('read-tail-r45.ts'))).toBe('7e85a1f2555ab7d26ef18662380522a8db6d4f78cac5f5be0a27a9d1f3b120ec');
    expect(sha(src('read-tail-r47.ts'))).toBe('7973cb62b2004263bc84593b15b1d5e5ce43dff50b7beea695fab3c6af4d2e6d');
    expect(sha(src('read-tail-r48.ts'))).toBe('3c49967c017a03df44136e588df5b9ca71abf04d49465be79c2c572a3cadc816');
    expect(sha(src('read-tail-r50.ts'))).toBe('a22440f40b969c713465c4ad55110805176fb27c70500cc4c7584889110d919e');
    expect(sha(src('package-obligations.ts'))).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(sha(src('read-tail-r23.ts'))).toBe('b1f282c6de197c6681cb53bea93075a89fd9d16684d7d039af4ffe44d0c5548e');
    expect(sha(src('read-tail-r25.ts'))).toBe('61c9ca5ddd4cf429652ba5f4b0f8130bc065150070f5f6bc29c8a723f9c94bb2');
    expect(sha(src('supplier-bills.ts'))).toBe('f1485ee0fc42475f3337626f561f97aa4ab1d1921b6df412bc97c12a7e534fef');
  });

  test('worktree SELECT count is 82 - 2 + 1 = 81', () => {
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
    expect(total).toBe(80); // r52: two by-id EXISTS heads (-2 +1 helper) 81 -> 80.
  });
});
