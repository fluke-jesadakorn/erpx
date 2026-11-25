import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd57PeopleIdGuard,
  ppRd57CompRevisionByEngagementSql,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r57-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const flat = (text: string) => text.replace(/\s+/g, ' ');
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

// Byte oracles copied verbatim from the six pre-r57 inline call sites at HEAD.
// The caller's predicate/ordering tail is passed through unchanged; the fixed
// head, owner kind, ? order and bind arity must stay byte-identical.
const peopleUser = 'EXISTS(SELECT 1 FROM hr_people WHERE id=? AND user_id IS DISTINCT FROM ?)';
const peopleVersionStatus = "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=? AND status!='archived')";
const peopleVersion = 'EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=?)';
const compHead = 'SELECT * FROM hr_compensation_revisions WHERE engagement_id=?';
const compApprovedOrders = compHead + " AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC";
const compStatesOrders = compHead + " AND status IN ('approved','superseded') ORDER BY effective_from DESC";
const compApproved = compHead + " AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)";

type Row = {
  file: string;
  render: () => string;
  literal: string;
  call: string;
  inline: string;
  binds: string;
};

// Binds are the original call-site arrays/tuples, in original order; ? count is checked.
const oracles: Row[] = [
  {
    file: 'commerce-expense-payment-effects.ts', render: () => ppRd57PeopleIdGuard(' AND user_id IS DISTINCT FROM ?'),
    literal: peopleUser, call: 'ppRd57PeopleIdGuard(" AND user_id IS DISTINCT FROM ?")',
    inline: '"EXISTS(SELECT 1 FROM hr_people WHERE id=? AND user_id IS DISTINCT FROM ?)"',
    binds: 'assertion(db,key,label,ppRd57PeopleIdGuard(" AND user_id IS DISTINCT FROM ?"),values)',
  },
  {
    file: 'people-directory.ts', render: () => ppRd57PeopleIdGuard(" AND version=? AND status!='archived'"),
    literal: peopleVersionStatus, call: `ppRd57PeopleIdGuard(" AND version=? AND status!='archived'")`,
    inline: `"EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=? AND status!='archived')"`,
    binds: `ppRd57PeopleIdGuard(" AND version=? AND status!='archived'"), [id, expectedVersion]`,
  },
  {
    file: 'people-directory.ts', render: () => ppRd57PeopleIdGuard(' AND version=?'),
    literal: peopleVersion, call: 'ppRd57PeopleIdGuard(" AND version=?")',
    inline: '"EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=?)"',
    binds: 'ppRd57PeopleIdGuard(" AND version=?"), [personId, expectedVersion]',
  },
  {
    file: 'people-evaluation.ts', render: () => ppRd57CompRevisionByEngagementSql(" AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC"),
    literal: compApprovedOrders, call: `ppRd57CompRevisionByEngagementSql(" AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC")`,
    inline: `"SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC"`,
    binds: `ppRd57CompRevisionByEngagementSql(" AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC")).bind(id, date, date2)`,
  },
  {
    file: 'people-evaluation.ts', render: () => ppRd57CompRevisionByEngagementSql(" AND status IN ('approved','superseded') ORDER BY effective_from DESC"),
    literal: compStatesOrders, call: `ppRd57CompRevisionByEngagementSql(" AND status IN ('approved','superseded') ORDER BY effective_from DESC")`,
    inline: `"SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('approved','superseded') ORDER BY effective_from DESC"`,
    binds: `ppRd57CompRevisionByEngagementSql(" AND status IN ('approved','superseded') ORDER BY effective_from DESC")).bind(id)`,
  },
  {
    file: 'people-evaluation.ts', render: () => ppRd57CompRevisionByEngagementSql(" AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)"),
    literal: compApproved, call: `ppRd57CompRevisionByEngagementSql(" AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)")`,
    inline: `"SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)"`,
    binds: `ppRd57CompRevisionByEngagementSql(" AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)")).bind(id, effectiveFrom, effectiveFrom2)`,
  },
];

describe('People Payroll read-tail r57 dedupe', () => {
  test('two owner-local builders render six byte-identical, read-only heads', () => {
    expect(oracles).toHaveLength(6);
    for (const o of oracles) {
      expect(o.render(), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
    }
    // Bind arity is carried by the caller's ? list; the fixed head never shifts it.
    expect((peopleUser.match(/\?/g) ?? []).length).toBe(2);
    expect((peopleVersionStatus.match(/\?/g) ?? []).length).toBe(2);
    expect((peopleVersion.match(/\?/g) ?? []).length).toBe(2);
    expect((compApprovedOrders.match(/\?/g) ?? []).length).toBe(3);
    expect((compStatesOrders.match(/\?/g) ?? []).length).toBe(1);
    expect((compApproved.match(/\?/g) ?? []).length).toBe(3);
    // Tails stay distinct: each head renders only with its own original tail.
    expect(peopleUser).not.toBe(peopleVersionStatus);
    expect(peopleVersionStatus).not.toBe(peopleVersion);
    expect(compApprovedOrders).not.toBe(compApproved);
    expect(compStatesOrders).toContain("status IN ('approved','superseded') ORDER BY effective_from DESC");
    expect(compApproved).toContain(compHead);
  });

  test('all six callers delegate and keep no inline copy or bind change', () => {
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.inline);
      expect(src(o.file), o.file + ' imports r57 builders').toContain("from './pp-read-tail-r57-sources'");
      expect(flat(src(o.file)), o.file + ' unchanged binds').toContain(flat(o.binds));
    }
    expect(uses('commerce-expense-payment-effects.ts', 'ppRd57PeopleIdGuard')).toBe(1);
    expect(uses('people-directory.ts', 'ppRd57PeopleIdGuard')).toBe(2);
    expect(uses('people-evaluation.ts', 'ppRd57CompRevisionByEngagementSql')).toBe(3);
    for (const name of ['commerce-expense-payment-effects.ts', 'people-directory.ts']) {
      expect(src(name), name + ' has no foreign r57 symbol').not.toMatch(/ppRd57(?!PeopleIdGuard)/);
    }
    expect(src('people-evaluation.ts'), 'people-evaluation has no foreign r57 symbol').not.toMatch(/ppRd57(?!CompRevisionByEngagementSql)/);
  });

  test('helper delegates both heads to the shared round-2c/2d builders', () => {
    const helper = src('pp-read-tail-r57-sources.ts');
    expect((helper.match(/export const ppRd57/g) ?? []).length).toBe(2);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
    expect(helper).toContain("from './pp-read-tail-round2c-sources'");
    expect(helper).toContain("ppRdRound2cStarByOnePred('hr_compensation_revisions', 'engagement_id', tail)");
    expect(helper).toContain("from './pp-read-tail-round2d-sources'");
    expect(helper).toContain("ppRdRound2dExistsById('hr_people', tail)");
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toContain('@erpx/server-');
  });

  test('r54/r55/r56 builders, advisory, locks, native kinds and cross-owner tails stay untouched', () => {
    expect(src('pp-read-tail-r54-sources.ts')).toContain('export const ppRd54RunEvidenceGuard');
    expect(src('pp-read-tail-r55-sources.ts')).toContain('export const ppRd55AttachmentPresenceV2');
    expect(src('pp-read-tail-r56-sources.ts')).toContain('export const ppRd56EngagementGuard');
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    // The unique-payment-slip cross-owner projection tail remains inline at its caller.
    expect(src('expense-payments.ts')).toContain("used.owner_type='expense_payment' AND used.revoked_at IS NULL");
    expect(src('expense-payments.ts')).toContain("current.owner_type='expense_payment' AND current.owner_id=?");
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 1202 (before 1206)', () => {
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
    expect(total).toBeLessThan(1206);
    expect(total).toBeLessThanOrEqual(1145);
  });
});
