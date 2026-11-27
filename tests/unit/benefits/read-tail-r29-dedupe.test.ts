import { afterEach, describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { benefitExistsHead, benefitSourceProbe } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';
import {
  benefitOwnerReadConfiguration,
  createBenefitOwnerReadClient,
  NATIVE_BENEFITS_OWNER_READ_CONFIG,
  nativeBenefitReadFirst,
  RD_ATTACHMENT_BENEFIT_DEPENDENTS,
  RD_ATTACHMENT_BENEFIT_ELECTIONS,
  readBenefitReadReply,
} from '../../../packages/server-benefits/src/adapters/native-benefit-read';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r29 fold. The byte-identical EXISTS
// head `EXISTS(SELECT 1 FROM ${table} ${tail})` established at r28 was still
// inline in the People-benefits package-settlement release guard. That guard now
// routes through the owner-local benefitExistsHead renderer. Rendered SQL, bind
// order, column references and source locks must be unchanged.
const electionTail = "event WHERE event.election_id=e.id AND event.version=handoff.source_election_version AND event.action='release' AND event.status='released' AND event.actor_id=handoff.released_by";
const preDedupe = `EXISTS(SELECT 1 FROM benefit_election_events ${electionTail})`;
const handoffPrefix = " OR win.id IS NULL OR win.company_id IS DISTINCT FROM e.company_id OR win.version<e.window_version\n OR (e.status='released' AND (e.released_by IS NULL OR NOT EXISTS(SELECT 1 FROM benefit_payroll_inputs handoff";
const handoffWhere = " WHERE handoff.election_id=e.id AND handoff.company_id=e.company_id AND handoff.person_id=e.person_id AND handoff.engagement_id=e.engagement_id AND handoff.currency=e.currency AND handoff.source_election_version<=e.version AND handoff.employee_deduction_minor=e.employee_deduction_minor AND handoff.employer_cost_minor=e.employer_cost_minor AND handoff.effective_from=e.effective_from AND handoff.effective_to IS NOT DISTINCT FROM e.effective_to AND ";

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
    }
  };
  walk(srcRoot);
  return total;
};

describe('benefits read-tail r29 exists-head extension dedupe', () => {
  test('the settlement EXISTS head and native-source probe share one SELECT-1 renderer', () => {
    expect(benefitExistsHead('benefit_plans', 'WHERE id=?'))
      .toBe('EXISTS(SELECT 1 FROM benefit_plans WHERE id=?)');
    expect(benefitSourceProbe('native_sources', 'benefit_source'))
      .toBe('SELECT 1 FROM (native_sources) benefit_source');
    const head = src('benefits-read-sql.ts');
    expect(head.match(/`SELECT 1 FROM \$\{source\}\$\{tail\}`/g) ?? []).toHaveLength(1);
    expect(head).toContain('selectOneFrom(`(${source})`, ` ${alias}`)');
    expect(head).toContain('`EXISTS(${selectOneFrom(table, ` ${tail}`)})`');
  });

  test('the release guard EXISTS head is byte-identical to the pre-dedupe literal', () => {
    const rendered = benefitExistsHead('benefit_election_events', electionTail);
    expect(rendered).toBe(preDedupe);
    expect(sha(rendered)).toBe('7347b2a7ce69c10afa3ac1ffdbc12a3ecc6dd4e177dc7dce108a5991e12fdd42');
    expect(Buffer.from(rendered)).toEqual(Buffer.from(preDedupe));
    expect(rendered.match(/\?/g) ?? []).toHaveLength(0);
    expect(rendered.match(/SELECT /g) ?? []).toHaveLength(1);
    readOnly(rendered);
  });

  test('the composed release handoff guard keeps its exact pre-dedupe bytes', () => {
    const before = `${handoffPrefix}${handoffWhere}${preDedupe})`;
    const after = `${handoffPrefix}${handoffWhere}${benefitExistsHead('benefit_election_events', electionTail)})`;
    expect(after).toBe(before);
    expect(sha(after)).toBe('5cc7340f02c6fe935b8693846eb24e58b8297b5016cab2c381696d660c6fd718');
    expect(Buffer.from(after)).toEqual(Buffer.from(before));
    expect(after.match(/SELECT/g) ?? []).toHaveLength(2);
    readOnly(after);
  });

  test('the renderer owns the single EXISTS head and the people guard delegates to it', () => {
    const renderer = src('benefits-read-sql.ts');
    expect(renderer).toContain('export const benefitExistsHead = (table: string, tail: string) =>');
    expect(renderer).toContain('`EXISTS(${selectOneFrom(table, ` ${tail}`)})`');
    expect(renderer.match(/`SELECT 1 FROM \$\{source\}\$\{tail\}`/g) ?? []).toHaveLength(1);
    const people = src('people-package-obligations.ts');
    expect(people).toContain('benefitExistsHead');
    expect(people).toContain("benefitExistsHead('benefit_election_events', `event WHERE");
    expect(people).not.toContain('EXISTS(SELECT 1 FROM benefit_election_events event WHERE');
    expect(people.match(/benefitExistsHead\(/g) ?? []).toHaveLength(1);
  });

  test('the fold adds no write, row-lock, FOR UPDATE, FOR SHARE, native registry or cross-owner consumer', () => {
    for (const name of ['benefits-read-sql.ts', 'people-package-obligations.ts']) {
      expect(src(name), name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE/i);
      // The only native_ token is the wave-3 doc comment naming the Rust reader
      // that succeeded the retired payroll-input SELECT; it carries no import.
      expect((src(name).match(/native_/g) ?? []).length, name).toBeLessThanOrEqual(name === 'benefits-read-sql.ts' ? 1 : 0);
      expect(src(name), name).not.toContain('@erpx/server-benefits/adapters/benefits-native-read');
    }
    for (const [name, count, mode] of [
      ['purchases.ts', 1, 'IN SHARE ROW EXCLUSIVE MODE'],
      ['package-obligations.ts', 1, 'IN SHARE MODE'],
      ['people-package-obligations.ts', 1, 'IN SHARE MODE'],
    ] as const) {
      expect(src(name).match(/LOCK\s+TABLE/g) ?? [], name).toHaveLength(count);
      expect(src(name), name).toContain(mode);
    }
  });

  test('retirement:ts-statements is 25, strictly below the r29 BEFORE=31 baseline', () => {
    expect(kpi()).toBe(25);
    expect(kpi()).toBeLessThan(31);
  });
});
