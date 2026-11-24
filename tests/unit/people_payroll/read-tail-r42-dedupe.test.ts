import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd42ActivePersonPoint,
  ppRd42FourColOwnerProjection,
  ppRd42IdVersionLiteralGuard,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r42-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

type Oracle = {
  file: string;
  call: string;
  literal: string;
  render: (args: string[]) => string;
  binds: string;
};

// Each literal is copied verbatim from the pre-r42 inline call site. Bind arrays are unchanged.
const oracles: Oracle[] = [
  {
    file: 'executive-report-sources.ts',
    call: 'ppRd42ActivePersonPoint("legal_name","hr_people","company_id","user_id")',
    literal: "SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND status='active' AND merged_into_id IS NULL",
    render: a => ppRd42ActivePersonPoint(a[0], a[1], a[2], a[3]),
    binds: '.bind(companyId, recipientId)',
  },
  {
    file: 'organization-authority-people.ts',
    call: 'ppRd42ActivePersonPoint("user_id","hr_people","id","company_id")',
    literal: "SELECT user_id FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL",
    render: a => ppRd42ActivePersonPoint(a[0], a[1], a[2], a[3]),
    binds: '.bind(personId,companyId)',
  },
  {
    file: 'beta-management-sources.ts',
    call: 'ppRd42FourColOwnerProjection("company_id,month,input_tokens,output_tokens","expense_ai_usage")',
    literal: 'SELECT company_id,month,input_tokens,output_tokens FROM expense_ai_usage',
    render: a => ppRd42FourColOwnerProjection(a[0], a[1]),
    binds: 'aiUsageSource:',
  },
  {
    file: 'people-purchases.ts',
    call: 'ppRd42FourColOwnerProjection("id,company_id,person_id,engagement_id","people_lifecycle_cases")',
    literal: 'SELECT id,company_id,person_id,engagement_id FROM people_lifecycle_cases',
    render: a => ppRd42FourColOwnerProjection(a[0], a[1]),
    binds: '.bind(target.id)',
  },
  {
    file: 'people-payroll.ts',
    call: 'ppRd42IdVersionLiteralGuard("hr_engagements","currency")',
    literal: 'EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND currency=?)',
    render: a => ppRd42IdVersionLiteralGuard(a[0], a[1]),
    binds: '[engagementId, version, currency]',
  },
  {
    file: 'people-payroll.ts',
    call: 'ppRd42IdVersionLiteralGuard("hr_compensation_revisions","status")',
    literal: 'EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status=?)',
    render: a => ppRd42IdVersionLiteralGuard(a[0], a[1]),
    binds: '[compensationId, expectedVersion, value2]',
  },
];

const argsOf = (call: string) => JSON.parse('[' + call.slice(call.indexOf('(') + 1, -1) + ']') as string[];

describe('People Payroll read-tail r42 three-family dedupe', () => {
  test('the three shared builders own byte-identical, read-only literals', () => {
    expect(ppRd42ActivePersonPoint('id', 't', 'a', 'b')).toBe("SELECT id FROM t WHERE a=? AND b=? AND status='active' AND merged_into_id IS NULL");
    expect(ppRd42FourColOwnerProjection('a,b,c,d', 't')).toBe('SELECT a,b,c,d FROM t');
    expect(ppRd42IdVersionLiteralGuard('t', 'status')).toBe('EXISTS(SELECT 1 FROM t WHERE id=? AND version=? AND status=?)');
    for (const o of oracles) {
      expect(o.render(argsOf(o.call)), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(6);
  });

  test('each caller uses its builder, keeps no inline copy and keeps its binds', () => {
    const shared = src('pp-read-tail-r42-sources.ts');
    expect((shared.match(/export const ppRd42/g) ?? []).length).toBe(3);
    for (const o of oracles) {
      const text = src(o.file);
      expect(text, o.file + ' builder call').toContain(o.call);
      expect(text, o.file + ' inline copy removed').not.toContain(o.literal);
      expect(text, o.file + ' binds unchanged').toContain(o.binds);
    }
    expect(uses('executive-report-sources.ts', 'ppRd42ActivePersonPoint')).toBe(1);
    expect(uses('organization-authority-people.ts', 'ppRd42ActivePersonPoint')).toBe(1);
    expect(uses('beta-management-sources.ts', 'ppRd42FourColOwnerProjection')).toBe(1);
    expect(uses('people-purchases.ts', 'ppRd42FourColOwnerProjection')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd42IdVersionLiteralGuard')).toBe(2);
  });

  test('locks, native kinds and r1-r41 builders stay untouched', () => {
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('payroll-purchases.ts')).toContain('FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('organization-authority-people.ts')).toContain("nativePpStatement('pp_organization_self_ceo')");
    expect(src('pp-read-tail-r36-sources.ts')).toContain('export const ppRd36TwoPredTail');
    expect(src('pp-read-tail-r32-sources.ts')).toContain('export const ppRd32TwoColProjection');
    expect(src('pp-read-tail-r40-sources.ts')).toContain('export const ppRd40UnlinkedUserByCompany');
    expect(src('pp-read-tail-r39-sources.ts')).toContain('export const ppRd39NumberedThreePred');
    expect(src('pp-read-tail-r38-sources.ts')).toContain('export const ppRd38SinglePredTail');
    expect(src('pp-read-tail-r37-sources.ts')).toContain('export const ppRd37IdVersionInGuard');
    expect(src('pp-read-tail-r33-sources.ts')).toContain('export const ppRd33FourColByIdCompany');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1306', () => {
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
    expect(total).toBeLessThan(1306);
    expect(total).toBe(1233);
  });
});
