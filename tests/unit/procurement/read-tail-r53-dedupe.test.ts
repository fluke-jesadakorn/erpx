import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { existsSourceWhereHead } from '../../../packages/server-procurement/src/adapters/read-tail-r53';
import { pointVersionGuard } from '../../../packages/server-procurement/src/adapters/read-tail-r19';
import { approvedRequestExists } from '../../../packages/server-procurement/src/adapters/read-tail-r34';

const adapters = (name: string) => new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFileSync(adapters(name), 'utf8');
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=80 source at HEAD
// a348ca7b11d99e3c7fe902d2d2a2a7e29167a7dc. `head` is the exact byte-identical
// SQL prefix the r53 renderer owns; each `tail` stays at the caller with every
// bind, JOIN and predicate fragment unchanged.
const HEAD = 'EXISTS(SELECT 1 FROM ${source} WHERE ';
const ORACLES = [
  {
    site: 'r19/rfqs warehouse point-version fence',
    source: '(${commercePlatformSources.commerce_warehouses}) commerce_warehouses',
    before: " AND status='active'",
    after: '',
    prefix: '', where: '', evidence: '', items: '',
    tail: "id=? AND company_id=? AND status='active' AND version=?)",
    sql: "EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_warehouses}) commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND version=?)",
  },
  {
    site: 'r19/rfqs supplier point-version fence',
    source: '(${financePlatformSources.finance_parties}) finance_parties',
    before: '',
    after: " AND status='active' AND currency=?",
    prefix: '', where: '', evidence: '', items: '',
    tail: "id=? AND company_id=? AND version=? AND status='active' AND currency=?)",
    sql: "EXISTS(SELECT 1 FROM (${financePlatformSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND version=? AND status='active' AND currency=?)",
  },
  {
    site: 'r34/rfqs source-evidence approval guard',
    source: '(${recordsPlatformSources.employee_requests}) e JOIN procurement_request_orders l ON l.request_id=e.id AND l.order_id=? AND l.rfq_id=?',
    before: '', after: '',
    prefix: 'e.',
    where: 'e.id=? AND e.company_id=?',
    evidence: '?', items: '?',
    tail: "e.id=? AND e.company_id=? AND e.evidence_version=? AND e.items_json=? AND e.approval_json::jsonb->>'decision'='approved')",
    sql: "EXISTS(SELECT 1 FROM (${recordsPlatformSources.employee_requests}) e JOIN procurement_request_orders l ON l.request_id=e.id AND l.order_id=? AND l.rfq_id=? WHERE e.id=? AND e.company_id=? AND e.evidence_version=? AND e.items_json=? AND e.approval_json::jsonb->>'decision'='approved')",
  },
  {
    site: 'r34/rfqs workspace source-ready approval guard',
    source: '(${recordsPlatformSources.employee_requests}) source',
    before: '', after: '',
    prefix: 'source.',
    where: "source.id=r.request_id AND source.company_id=r.company_id AND source.state='in_progress'",
    evidence: 'r.request_evidence_version', items: 'r.request_items_json',
    tail: "source.id=r.request_id AND source.company_id=r.company_id AND source.state='in_progress' AND source.evidence_version=r.request_evidence_version AND source.items_json=r.request_items_json AND source.approval_json::jsonb->>'decision'='approved')",
    sql: "EXISTS(SELECT 1 FROM (${recordsPlatformSources.employee_requests}) source WHERE source.id=r.request_id AND source.company_id=r.company_id AND source.state='in_progress' AND source.evidence_version=r.request_evidence_version AND source.items_json=r.request_items_json AND source.approval_json::jsonb->>'decision'='approved')",
  },
];

// BEFORE sha256 of the two edited builder files. Reversing both folds
// (renderer call -> original literal, import/comment restored) must reproduce
// them exactly, proving no rendered byte was lost.
const BEFORE: Record<string, string> = {
  'read-tail-r19.ts': '2a5d029126532ce89ebe4e9de1dd3ffb3e8950d75a640a823804d189ba27f21f',
  'read-tail-r34.ts': 'e110f9feb6f99aafbd7e2424aa552fbd35e2d0c479c5afd1f798d2714fd7348d',
};
const revert: Record<string, (s: string) => string> = {
  'read-tail-r19.ts': (s) => s
    .replace("import {existsSourceWhereHead} from './read-tail-r53';\n", '')
    .replace("/** Point version fence. The byte-identical EXISTS/FROM/WHERE head now comes from\n * r53; the id/company/version tail, binds and bind order stay here. */\n", '')
    .replace(
      '`${existsSourceWhereHead(source)}id=? AND company_id=?${before} AND version=?${after})`',
      '`EXISTS(SELECT 1 FROM ${source} WHERE id=? AND company_id=?${before} AND version=?${after})`',
    ),
  'read-tail-r34.ts': (s) => s
    // r54 fold is reversed first, then the r53 fold, to reach the BEFORE bytes.
    .replace("import {columnsFromWhereHead} from './read-tail-r54';\n", '')
    .replace(' * LIMIT bind stays at each call site. The byte-identical columns/FROM/WHERE head\n * now comes from r54. */', ' * LIMIT bind stays at each call site. */')
    .replace(
      ') => columnsFromWhereHead(columns, from, `${branchScope} AND ${scopeExpr} AND ${predicates} AND ${scopeWhere} ORDER BY ${order} LIMIT ?`);',
      ') => `SELECT ${columns} FROM ${from} WHERE ${branchScope} AND ${scopeExpr} AND ${predicates} AND ${scopeWhere} ORDER BY ${order} LIMIT ?`;',
    )
    .replace("import {existsSourceWhereHead} from './read-tail-r53';\n", '')
    .replace(
      ' * evidence/items witnesses render verbatim; binds stay at each call site. The\n * byte-identical EXISTS/FROM/WHERE head now comes from r53. */',
      ' * evidence/items witnesses render verbatim; binds stay at each call site. */',
    )
    .replace(
      "`${existsSourceWhereHead(sourceExpr)}${where} AND ${prefix}evidence_version=${evidence} AND ${prefix}items_json=${items} AND ${prefix}approval_json::jsonb->>'decision'='approved')`",
      "`EXISTS(SELECT 1 FROM ${sourceExpr} WHERE ${where} AND ${prefix}evidence_version=${evidence} AND ${prefix}items_json=${items} AND ${prefix}approval_json::jsonb->>'decision'='approved')`",
    ),
};

describe('Procurement read-tail r53 EXISTS source/WHERE head dedupe', () => {
  test('the renderer owns exactly one SELECT and emits the byte-identical head', () => {
    const helper = src('read-tail-r53.ts');
    expect(count(helper, 'export const existsSourceWhereHead =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(existsSourceWhereHead('${source}')).toBe(HEAD);
    expect(existsSourceWhereHead('T')).toBe('EXISTS(SELECT 1 FROM T WHERE ');
  });

  test('each head plus its original tail renders byte-identically', () => {
    for (const { site, source, tail, sql } of ORACLES) {
      expect([site, existsSourceWhereHead(source) + tail]).toEqual([site, sql]);
    }
  });

  test('both exported builders keep their exact original rendered SQL', () => {
    expect(pointVersionGuard(ORACLES[0].source, ORACLES[0].before, ORACLES[0].after)).toBe(ORACLES[0].sql);
    expect(pointVersionGuard(ORACLES[1].source, ORACLES[1].before, ORACLES[1].after)).toBe(ORACLES[1].sql);
    expect(approvedRequestExists(ORACLES[2].source, ORACLES[2].prefix, ORACLES[2].where, ORACLES[2].evidence, ORACLES[2].items)).toBe(ORACLES[2].sql);
    expect(approvedRequestExists(ORACLES[3].source, ORACLES[3].prefix, ORACLES[3].where, ORACLES[3].evidence, ORACLES[3].items)).toBe(ORACLES[3].sql);
  });

  test('both builders delegate to r53 and the retired inline heads are gone', () => {
    const r19 = src('read-tail-r19.ts');
    const r34 = src('read-tail-r34.ts');
    for (const s of [r19, r34]) expect(s).toContain("import {existsSourceWhereHead} from './read-tail-r53';");
    expect(count(r19, 'existsSourceWhereHead(')).toBe(1);
    expect(count(r34, 'existsSourceWhereHead(')).toBe(1);
    expect(r19).not.toContain('`EXISTS(SELECT 1 FROM ${source} WHERE id=?');
    expect(r34).not.toContain('`EXISTS(SELECT 1 FROM ${sourceExpr} WHERE ');
    // Call sites keep their binds and ordering fragments untouched.
    expect(src('rfqs.ts')).toContain('pointVersionGuard(');
    expect(src('rfqs.ts')).toContain('approvedRequestExists(');
    expect(src('attachments-service-facts.ts')).toContain('pointVersionGuard(');
    expect(src('attachments-operational-facts.ts')).toContain('pointVersionGuard(');
  });

  test('reversing both folds reproduces the original source bytes', () => {
    for (const [file, before] of Object.entries(BEFORE)) {
      expect([file, sha(revert[file](src(file)))]).toEqual([file, before]);
    }
  });

  test('no write, lock, native or cross-owner consumer was touched', () => {
    expect(src('read-tail-r53.ts')).not.toMatch(/FOR\s+(UPDATE|SHARE)|LOCK\s+TABLE|INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/i);
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
    expect(sha(src('read-tail-r47.ts'))).toBe('7973cb62b2004263bc84593b15b1d5e5ce43dff50b7beea695fab3c6af4d2e6d');
    expect(sha(src('read-tail-r48.ts'))).toBe('3c49967c017a03df44136e588df5b9ca71abf04d49465be79c2c572a3cadc816');
    expect(sha(src('read-tail-r50.ts'))).toBe('a22440f40b969c713465c4ad55110805176fb27c70500cc4c7584889110d919e');
    expect(sha(src('read-tail-r51.ts'))).toBe('08bd16f0507ba84fe3f9bf6285faea9c24126a871141c78707dcbdeb3f907958');
    expect(sha(src('read-tail-r52.ts'))).toBe('3b588e2a3ec73e6ac8caf5eee864c1b8e5b95139e9eb40da12e732c31261603d');
    expect(sha(src('package-obligations.ts'))).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(sha(src('supplier-bills.ts'))).toBe('f1485ee0fc42475f3337626f561f97aa4ab1d1921b6df412bc97c12a7e534fef');
  });

  test('worktree SELECT count is 80 - 2 + 1 = 79, then r54 79 - 2 + 1 = 78', () => {
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
    expect(total).toBeLessThan(80);
    expect(total).toBe(77); // r54: two columns/FROM/WHERE heads (-2 +1 helper) 79 -> 78; r55: two ARRAY heads (-2 +1 helper) 78 -> 77.
  });
});
