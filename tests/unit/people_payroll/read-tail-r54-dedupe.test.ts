import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd54RunEvidenceGuard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r54-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).toMatch(/^EXISTS\(/);
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const flat = (text: string) => text.replace(/\s+/g, ' ');

// Byte oracles captured verbatim from the pre-r54 inline call sites at HEAD.
// Each tail split is the exact argument pair the site now passes to the builder.
const g147 = "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='approved' AND version=? AND input_digest IS NOT NULL)";
const g174 = "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND revision=? AND input_digest=? AND status IN ('preview','in_review','approved'))";
const g14 = "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','failed','preview') AND submitted_by IS NULL)";
const g23 = "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','preview','failed') AND (submitted_by IS NULL OR status='calculating' AND approved_by IS NULL))";

type Row = {
  file: string;
  guard: string;
  tail: [string, string];
  literal: string;
  binds: string[];
  call: string;
  bindCall: string;
};

// Binds are the original arrays in original order; ? count is checked against them.
const oracles: Row[] = [
  {
    file: 'people-payroll.ts', guard: 'guard147',
    tail: ["status='approved' AND version=? AND input_digest", ' IS NOT NULL'],
    literal: g147, binds: ['runId', 'companyId', 'expectedVersion'],
    call: 'ppRd54RunEvidenceGuard("status=\'approved\' AND version=? AND input_digest", " IS NOT NULL")',
    bindCall: '[runId, companyId, expectedVersion]',
  },
  {
    file: 'people-payroll.ts', guard: 'guard174',
    tail: ["revision=? AND input_digest=? AND status IN ('preview','in_review','approved')", ''],
    literal: g174, binds: ['runId', 'companyId', 'revision', 'input_digest'],
    call: "ppRd54RunEvidenceGuard(\"revision=? AND input_digest=? AND status IN ('preview','in_review','approved')\", \"\")",
    bindCall: '[runId, companyId, revision, input_digest]',
  },
  {
    file: 'payroll-adjustments.ts', guard: 'guard14',
    tail: ["version=? AND status IN ('draft','calculating','failed','preview') AND submitted_by", ' IS NULL'],
    literal: g14, binds: ['runId', 'companyId', 'expectedVersion'],
    call: "ppRd54RunEvidenceGuard(\"version=? AND status IN ('draft','calculating','failed','preview') AND submitted_by\", \" IS NULL\")",
    bindCall: '[runId, companyId, expectedVersion]',
  },
  {
    file: 'payroll-adjustments.ts', guard: 'guard23',
    tail: ["version=? AND status IN ('draft','calculating','preview','failed') AND (submitted_by IS NULL OR status='calculating' AND approved_by IS NULL)", ''],
    literal: g23, binds: ['id', 'companyId', 'version'],
    call: "ppRd54RunEvidenceGuard(\"version=? AND status IN ('draft','calculating','preview','failed') AND (submitted_by IS NULL OR status='calculating' AND approved_by IS NULL)\", \"\")",
    bindCall: '[id, companyId, version]',
  },
];

describe('People Payroll read-tail r54 dedupe', () => {
  test('the shared builder renders four byte-identical, read-only, EXISTS-only guards', () => {
    for (const o of oracles) {
      expect(ppRd54RunEvidenceGuard(...o.tail), o.file + ' ' + o.guard).toBe(o.literal);
      expect((o.literal.match(/\?/g) ?? []).length, o.guard + ' bind arity').toBe(o.binds.length);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(4);
    // Exact status-list ordering and NULL tails are preserved, not normalized.
    expect(g14).toContain("('draft','calculating','failed','preview')");
    expect(g23).toContain("('draft','calculating','preview','failed')");
    expect(g14).not.toContain("('draft','calculating','preview','failed')");
    expect(g23).not.toContain("('draft','calculating','failed','preview')");
    expect(g147).toContain('input_digest IS NOT NULL)');
    expect(g174).toContain("status IN ('preview','in_review','approved'))");
    // Single canonical builder, single SELECT, no lock or native leakage.
    const helper = src('pp-read-tail-r54-sources.ts');
    expect((helper.match(/export const ppRd54/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|nativePpStatement/i);
  });

  test('all four callers delegate to the builder and keep no inline copy or bind change', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' ' + o.guard + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' ' + o.guard + ' inline copy removed').not.toContain(o.literal);
      expect(src(o.file), o.file + ' imports r54 builder').toContain("from './pp-read-tail-r54-sources'");
      expect(flat(src(o.file)), o.guard + ' bind array').toContain(flat(o.bindCall));
    }
    expect(uses('people-payroll.ts', 'ppRd54RunEvidenceGuard')).toBe(2);
    expect(uses('payroll-adjustments.ts', 'ppRd54RunEvidenceGuard')).toBe(2);
    for (const name of ['people-payroll.ts', 'payroll-adjustments.ts']) {
      expect(src(name), name + ' has no foreign r54 symbol').not.toMatch(/ppRd54(?!RunEvidenceGuard)/);
    }
  });

  test('r21/r24/r37 guards, locks and native sources stay byte-identical and untouched', () => {
    expect(src('pp-read-tail-r21-sources.ts')).toContain("`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND status='${status}' AND version=?)`");
    expect(src('pp-read-tail-r21-sources.ts')).toContain("`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND status='${status}')`");
    expect(src('pp-read-tail-r24-sources.ts')).toContain('EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status=\'approved\' AND version=? AND revision=? AND input_digest=?)');
    expect(src('pp-read-tail-r37-sources.ts')).toContain('`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND ${c2}=? AND status=\'${status}\' AND ${c4}=? AND ${c5}=?)`');
    expect(src('pp-read-tail-r37-sources.ts')).toContain('`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=? AND ${col} IN (${placeholders}))`');
    expect(src('pp-read-tail-r37-sources.ts')).toContain('`EXISTS(SELECT 1 FROM hr_payroll_recoveries WHERE id=? AND company_id=? AND version=? AND status=\'${status}\' AND ${c5}=?)`');
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT id FROM hr_payroll_runs WHERE id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('FOR SHARE');
    expect(src('payroll-adjustments.ts')).toContain('nativePpStatement');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1217', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
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
    expect(total).toBeLessThan(1217);
    expect(total).toBe(1175);
  });
});
