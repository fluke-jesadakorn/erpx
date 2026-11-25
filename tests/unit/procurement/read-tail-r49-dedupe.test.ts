import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { sourceImage } from '../../../packages/server-procurement/src/adapters/read-tail-r31';
import { branchDepartmentScope } from '../../../packages/server-procurement/src/adapters/read-tail-sql';
import { platformOwnerProjectionSources as commercePlatformSources } from '../../../packages/server-commerce/src/public/platform-owner-projection-sources';
import { platformOwnerProjectionSources as financePlatformSources } from '../../../packages/server-finance/src/public/platform-owner-projection-sources';
import { platformOwnerProjectionSources as people_payrollPlatformSources } from '../../../packages/server-people_payroll/src/public/platform-owner-projection-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (name: string) => createHash('sha256').update(src(name)).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=88 rendered SQL at
// HEAD e313f4375f2100a8db74a4851395530db948f3ad. `sql` is the exact string
// handed to write.prepare at each of the three sites before the r49 fold.
const scope = branchDepartmentScope('o');
const ORACLES = [
  {
    site: 'supplier-bills/policy',
    image: 'p.*,COALESCE(person.legal_name,p.updated_by) updated_by_name',
    from: `procurement_invoice_policies p LEFT JOIN (${people_payrollPlatformSources.hr_people}) person ON person.company_id=p.company_id AND person.user_id=p.updated_by AND person.merged_into_id IS NULL`,
    where: 'p.company_id=?',
    sql: `SELECT p.*,COALESCE(person.legal_name,p.updated_by) updated_by_name FROM procurement_invoice_policies p LEFT JOIN (${people_payrollPlatformSources.hr_people}) person ON person.company_id=p.company_id AND person.user_id=p.updated_by AND person.merged_into_id IS NULL WHERE p.company_id=?`,
  },
  {
    site: 'supplier-bills/receipt',
    image: 'f.*,j.status journal_status,j.currency journal_currency,j.chart_id journal_chart_id',
    from: `(${commercePlatformSources.commerce_fulfillments}) f LEFT JOIN (${financePlatformSources.finance_journals}) j ON j.id=f.journal_id`,
    where: 'f.id=?',
    sql: `SELECT f.*,j.status journal_status,j.currency journal_currency,j.chart_id journal_chart_id FROM (${commercePlatformSources.commerce_fulfillments}) f LEFT JOIN (${financePlatformSources.finance_journals}) j ON j.id=f.journal_id WHERE f.id=?`,
  },
  {
    site: 'supplier-promises/order',
    image: 'o.*,p.name supplier_name',
    from: `(${commercePlatformSources.commerce_orders}) o JOIN (${financePlatformSources.finance_parties}) p ON p.id=o.party_id`,
    where: `o.id=? AND o.kind='purchase' AND ${scope}`,
    sql: `SELECT o.*,p.name supplier_name FROM (${commercePlatformSources.commerce_orders}) o JOIN (${financePlatformSources.finance_parties}) p ON p.id=o.party_id WHERE o.id=? AND o.kind='purchase' AND ${scope}`,
  },
];

describe('Procurement read-tail r49 supplier read-head dedupe', () => {
  test('all three heads render byte-identically through sourceImage', () => {
    for (const { site, image, from, where, sql } of ORACLES) {
      expect([site, sourceImage(image, from, where)]).toEqual([site, sql]);
    }
  });

  test('the helper owns the single SELECT head and was not edited', () => {
    const h = src('read-tail-r31.ts');
    expect(count(h, 'export const sourceImage =')).toBe(1);
    expect(h).toContain('`SELECT ${image} FROM ${from} WHERE ${where}`');
    expect(sha('read-tail-r31.ts')).toBe('d6d1fa0a6b3499731d60d8cb9aad71f2f9bfde5f19b77ea354d4016aae432ce1');
    expect(sourceImage('a', 't x', 'id=?')).toBe('SELECT a FROM t x WHERE id=?');
  });

  test('bill and promise sites delegate without moving binds', () => {
    const b = src('supplier-bills.ts');
    const p = src('supplier-promises.ts');
    expect(b).toContain("import {sourceImage} from './read-tail-r31';");
    expect(p).toContain("import {sourceImage} from './read-tail-r31';");
    expect(count(b, '.prepare(sourceImage(')).toBe(2);
    expect(count(p, '.prepare(sourceImage(')).toBe(1);
    expect(b).toContain(')).bind(actor.companyId).first<Row>()');
    expect(b).toContain(')).bind(id).first<Row>()');
    expect(p).toContain(')).bind(id,...scopeArgs(a)).first<Row>()');
    expect(b).not.toContain('SELECT p.*,COALESCE(person.legal_name');
    expect(b).not.toContain('SELECT f.*,j.status journal_status');
    expect(p).not.toContain('SELECT o.*,p.name supplier_name');
    for (const { image } of ORACLES) expect(b + p).toContain(image);
  });

  test('witnesses, locks, r47/r48 helpers and LOCK TABLE stay untouched', () => {
    expect(sha('read-tail-r28.ts')).toBe('d9a93d385847a379c5e470da3add3fd5e504558ec2a1e9c9fb94043efeab94d0');
    expect(sha('read-tail-r44.ts')).toBe('f5dc17c41403aded63b04458839a97a5db2ad0025139f88390478638f45239be');
    expect(sha('read-tail-r45.ts')).toBe('7e85a1f2555ab7d26ef18662380522a8db6d4f78cac5f5be0a27a9d1f3b120ec');
    expect(sha('read-tail-r47.ts')).toBe('7973cb62b2004263bc84593b15b1d5e5ce43dff50b7beea695fab3c6af4d2e6d');
    expect(sha('read-tail-r48.ts')).toBe('3c49967c017a03df44136e588df5b9ca71abf04d49465be79c2c572a3cadc816');
    expect(sha('package-obligations.ts')).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(src('read-tail-r28.ts')).toContain('export const attachmentVersionWitness =');
    expect(src('read-tail-r44.ts')).toContain('export const commandReceiptEventWitness =');
    expect(src('read-tail-r45.ts')).toContain('export const procurementAttachmentReadOuterFrame =');
    expect(src('read-tail-r47.ts')).toContain('export const procurementSettlementFactSelect =');
    expect(src('read-tail-r48.ts')).toContain('export const supplierDetailSelect =');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE procurement_rfqs,');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
  });

  test('worktree SELECT count is 88 - 3 = 85', () => {
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
    expect(total).toBeLessThan(88);
    expect(total).toBe(78); // r50: four bill-receipt heads (-4 +1 helper) 85 -> 82; r51: two jsonb_agg heads (-2 +1 helper) 82 -> 81; r52: two by-id EXISTS heads (-2 +1 helper) 81 -> 80; r53: two EXISTS/FROM/WHERE heads (-2 +1 helper) 80 -> 79; r54: two columns/FROM/WHERE heads (-2 +1 helper) 79 -> 78.
  });
});
