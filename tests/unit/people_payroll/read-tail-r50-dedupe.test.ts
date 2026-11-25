import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd50CoalesceDisplayName,
  ppRd50SlipOwnerGuard,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r50-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

// Verbatim owner projections. Byte-identity oracles only; never live query sources.
const orgFrom = 'FROM (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes) organization_nodes';
const attachments = 'SELECT "id","upload_id","owner_type","owner_id","purpose","object_key","revision","original_name","media_type","bytes","checksum_sha256","created_by","legal_hold","created_at","revoked_at","company_id","detected_media_type","source_state","scanned_at","scanner_definition_at","preview_object_key","preview_media_type","preview_checksum_sha256","preview_bytes","destroyed_at","legacy_sanitized_only" FROM attachments';

const displaySql = (alias: string, col: string, fallback: string) =>
  `COALESCE((SELECT display_name ${orgFrom} WHERE id=${alias}.${col} AND company_id=${alias}.company_id),${fallback})`;
const slipSql = (ownerType: string, ownerTypeFirst: boolean) =>
  `EXISTS(SELECT 1 FROM (${attachments}) attachments WHERE id=? AND ${ownerTypeFirst ? `owner_type='${ownerType}' AND owner_id=?` : `owner_id=? AND owner_type='${ownerType}'`} AND revoked_at IS NULL)`;
const staffLevelsSql = "`COALESCE((SELECT 'Level '||level_number||' · '||name ${organizationFinalSources.executive_metric_projections_staff_levels_from} WHERE id=e.level_id AND company_id=e.company_id),'Unmapped')`";

const render = {
  ppRd50CoalesceDisplayName: (a: string[]) => ppRd50CoalesceDisplayName(a[0], a[1], a[2], orgFrom),
  ppRd50SlipOwnerGuard: (a: string[]) => ppRd50SlipOwnerGuard(a[0], a[1] === 'true'),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; call: string };

// Each literal is copied from the pre-r50 inline call site in canonical builder form.
// No bind was introduced, moved or reordered; the fallback strings and owner order stay exact.
const oracles: Row[] = [
  {
    file: 'executive-metric-projections.ts',
    sym: 'ppRd50CoalesceDisplayName',
    args: ['e', 'branch_id', "'Unassigned'"],
    literal: displaySql('e', 'branch_id', "'Unassigned'"),
    call: "ppRd50CoalesceDisplayName('e', `${breakdown}_id`, \"'Unassigned'\", organizationFinalSources.executive_metric_projections_organization_nodes_from)",
  },
  {
    file: 'executive-metric-projections.ts',
    sym: 'ppRd50CoalesceDisplayName',
    args: ['l', 'branch_id', "'Unassigned branch'"],
    literal: displaySql('l', 'branch_id', "'Unassigned branch'"),
    call: "ppRd50CoalesceDisplayName('l', 'branch_id', \"'Unassigned branch'\", organizationFinalSources.executive_metric_projections_organization_nodes_from)",
  },
  {
    file: 'executive-metric-projections.ts',
    sym: 'ppRd50CoalesceDisplayName',
    args: ['l', 'department_id', "'Unassigned department'"],
    literal: displaySql('l', 'department_id', "'Unassigned department'"),
    call: "ppRd50CoalesceDisplayName('l', 'department_id', \"'Unassigned department'\", organizationFinalSources.executive_metric_projections_organization_nodes_from)",
  },
  {
    file: 'executive-metric-projections.ts',
    sym: 'ppRd50CoalesceDisplayName',
    args: ['o', 'branch_id', "'Unassigned branch'"],
    literal: displaySql('o', 'branch_id', "'Unassigned branch'"),
    call: "ppRd50CoalesceDisplayName('o', 'branch_id', \"'Unassigned branch'\", organizationFinalSources.executive_metric_projections_organization_nodes_from)",
  },
  {
    file: 'executive-metric-projections.ts',
    sym: 'ppRd50CoalesceDisplayName',
    args: ['o', 'department_id', "'Unassigned department'"],
    literal: displaySql('o', 'department_id', "'Unassigned department'"),
    call: "ppRd50CoalesceDisplayName('o', 'department_id', \"'Unassigned department'\", organizationFinalSources.executive_metric_projections_organization_nodes_from)",
  },
  {
    file: 'expense-payments.ts',
    sym: 'ppRd50SlipOwnerGuard',
    args: ['expense_payment', 'true'],
    literal: slipSql('expense_payment', true),
    call: "ppRd50SlipOwnerGuard('expense_payment', true)",
  },
  {
    file: 'expenses.ts',
    sym: 'ppRd50SlipOwnerGuard',
    args: ['expense_claim'],
    literal: slipSql('expense_claim', false),
    call: "ppRd50SlipOwnerGuard('expense_claim')",
  },
];

describe('People Payroll read-tail r50 two-family dedupe', () => {
  test('the two shared builders own byte-identical, read-only literals and bind counts', () => {
    expect(ppRd50CoalesceDisplayName('e', 'branch_id', "'Unassigned'", orgFrom)).toBe(displaySql('e', 'branch_id', "'Unassigned'"));
    expect(ppRd50SlipOwnerGuard('expense_payment', true)).toBe(slipSql('expense_payment', true));
    expect(ppRd50SlipOwnerGuard('expense_claim')).toBe(slipSql('expense_claim', false));
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.call).toBe(o.literal);
      readOnly(o.literal);
      expect((o.literal.match(/\?/g) ?? []).length, o.file + ' bind count').toBe(o.sym === 'ppRd50CoalesceDisplayName' ? 0 : 2);
      if (o.sym === 'ppRd50CoalesceDisplayName') {
        expect(o.literal).toContain(`SELECT display_name ${orgFrom}`);
        expect(o.literal).toContain(`company_id=${o.args[0]}.company_id`);
        expect(o.literal.endsWith(`,${o.args[2]})`)).toBe(true);
      } else {
        expect(o.literal).toContain(`owner_type='${o.args[0]}'`);
        expect(o.literal).toContain('revoked_at IS NULL');
      }
    }
    expect(oracles.length).toBe(7);
    // Three distinct exact fallback strings, each paired with its exact alias/column.
    expect(new Set(oracles.filter(o => o.sym === 'ppRd50CoalesceDisplayName').map(o => o.args[2]))).toEqual(
      new Set(["'Unassigned'", "'Unassigned branch'", "'Unassigned department'"]),
    );
    // The L20 staff_levels variant is not folded into the display-name builder.
    expect(src('executive-metric-projections.ts')).toContain(staffLevelsSql);
    expect(src('executive-metric-projections.ts')).toContain('organizationFinalSources.executive_metric_projections_staff_levels_from');
  });

  test('each caller uses its builder, keeps no inline head and keeps its binds and owner order', () => {
    expect((src('pp-read-tail-r50-sources.ts').match(/export const ppRd50/g) ?? []).length).toBe(2);
    const expectedCounts: Record<string, [Sym, number][]> = {
      'executive-metric-projections.ts': [['ppRd50CoalesceDisplayName', 5]],
      'expense-payments.ts': [['ppRd50SlipOwnerGuard', 1]],
      'expenses.ts': [['ppRd50SlipOwnerGuard', 1]],
    };
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
      expect(src(o.file), o.file + ' inline copy removed').not.toContain(o.literal);
      expect(src(o.file), o.file + ' imports r50 builders').toContain("from './pp-read-tail-r50-sources'");
    }
    for (const [name, pairs] of Object.entries(expectedCounts)) {
      for (const [sym, count] of pairs) expect(uses(name, sym), name + ' ' + sym).toBe(count);
    }
    expect(src('executive-metric-projections.ts')).not.toContain('COALESCE((SELECT display_name');
    // The two binds stay (attachmentId, id) and each owner_type keeps its own predicate order.
    expect(src('expense-payments.ts')).toContain(`"slip-owner",\n        ppRd50SlipOwnerGuard('expense_payment', true),\n        [attachmentId, id],`);
    expect(src('expenses.ts')).toContain(`"receipt",\n        ppRd50SlipOwnerGuard('expense_claim'),\n        [attachmentId, id],`);
    expect(src('expense-payments.ts')).not.toContain("WHERE id=? AND owner_type='expense_payment' AND owner_id=? AND revoked_at IS NULL");
    expect(src('expenses.ts')).not.toContain("WHERE id=? AND owner_id=? AND owner_type='expense_claim' AND revoked_at IS NULL");
  });

  test('locks, advisory, native kinds, cross-owner projections and r1-r49 builders stay untouched', () => {
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT id FROM hr_time_entries WHERE id=? FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('lockProfile(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")');
    expect(src('payroll-statutory.ts')).toContain('lockYtd(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))")');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('expenses.ts')).toContain("financeProjectionSources.finance_journals}) finance_journals WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=? AND status='posted'");
    expect(src('expense-payments.ts')).toContain("a.owner_type='expense_payment' AND a.owner_id=?");
    expect(src('executive-metric-projections.ts')).toContain('financeFinalSources.executive_metric_projections_finance_journals_j_join');
    expect(src('pp-read-tail-r49-sources.ts')).toContain('export const ppRd49ExpenseClaimReceiptV2');
    expect(src('pp-read-tail-r49-sources.ts')).toContain('hospitalityOwnerProjectionSources as filesProjectionSources');
    expect(src('pp-read-tail-r48-sources.ts')).toContain('hospitalityOwnerProjectionSources as financeProjectionSources');
    for (const name of ['executive-metric-projections.ts', 'expense-payments.ts', 'expenses.ts']) {
      expect(src(name), name + ' has no foreign r50 symbol').not.toMatch(/ppRd50(?!CoalesceDisplayName|SlipOwnerGuard)/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1233', () => {
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
    expect(total).toBeLessThan(1233);
    expect(total).toBe(1214);
  });
});
