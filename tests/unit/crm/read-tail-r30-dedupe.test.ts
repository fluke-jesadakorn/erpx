import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSelectBodySql } from '../../../modules/crm/server/src/adapters/read-tail-r46-sql';
import { crmCurrentScopeAllowedSql } from '../../../modules/crm/server/src/adapters/read-tail-r30-sql';

const owner = resolve(__dirname, '../../../modules/crm/server/src');
const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
const totalSelects = () => {
  let total = 0;
  for (const rel of readdirSync(owner, { recursive: true })) {
    if (!String(rel).endsWith('.ts')) continue;
    total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
  }
  return total;
};

const A = 'ASSIGN', S = 'SCOPE';
const MODULE = 'AND MOD?::text)))';
const METRIC = ` AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('crm'::text)))`;

/** Retired inline SQL, exactly as it read before r30. */
const retired: Array<[string, string, string]> = [
  ['analytics report-scope with module', crmCurrentScopeAllowedSql(A, S, 'report_scope', ` ${MODULE}`),
    `SELECT ${A} ${MODULE} allowed FROM ${S} report_scope`],
  ['analytics report-scope without module', crmCurrentScopeAllowedSql(A, S, 'report_scope', ' '),
    `SELECT ${A}  allowed FROM ${S} report_scope`],
  ['metric crm_metric_scope', crmCurrentScopeAllowedSql(A, S, 'crm_metric_scope', METRIC),
    `SELECT ${A}${METRIC} allowed FROM ${S} crm_metric_scope`],
];

describe('crm read-tail r30 byte-identical read family', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns the renderer once and composes it at both retired call sites', () => {
    const helper = src('read-tail-r30-sql.ts');
    expect(helper.match(/crmCurrentScopeAllowedSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);

    expect(src('analytics-purchases.ts')).toContain("crmCurrentScopeAllowedSql(assignmentReadSql('report_scope',permission),actorScopeSql,'report_scope'");
    expect(src('metric-purchases.ts')).toContain('crmCurrentScopeAllowedSql(assignmentReadSql("crm_metric_scope","commerce.read"),actorScopeSql,"crm_metric_scope"');

    expect(src('analytics-purchases.ts')).not.toContain('allowed FROM ${actorScopeSql} report_scope');
    expect(src('metric-purchases.ts')).not.toContain('allowed FROM ${actorScopeSql} crm_metric_scope');
  });

  test('leaves locks, command receipts, native kinds and r8-r29 helpers untouched', () => {
    const purchases = src('purchases.ts'), analytics = src('analytics-purchases.ts'), ledger = src('leads.ts');
    expect(purchases).toContain('const crmAssignmentAllowedSql=(alias:string,permission:string)=>crmCurrentScopeAllowedSql(assignmentReadSql(alias,permission),actorScopeSql,alias);');
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain("crmSelectExprSql(\"jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?\")");
    expect(purchases).toContain('export const crmAssignmentModuleExists=');
    expect(analytics).toContain('crmAssignmentModuleExists');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_17']) expect(ledger).toContain(kind);
    for (const [file, token] of [
      ['read-tail-r24-sql.ts', 'export const crmCreatorLeadScopeSql'],
      ['read-tail-r25-sql.ts', 'export const crmPurchaseRowProjection'],
      ['read-tail-r26-sql.ts', 'export const crmGuestLinkReservationSourceSql'],
      ['read-tail-r27-sql.ts', 'const crmOpportunityOwnerAnchor ='],
      ['read-tail-r28-sql.ts', 'export const crmCaseEventReadSql'],
      ['read-tail-r29-sql.ts', 'const crmAccountReadAnchor ='],
    ] as const) expect(src(file)).toContain(token);
  });

  test.each([
    [A, S, 'report_scope', ` ${MODULE}`],
    [A, S, 'report_scope', ' '],
    [A, S, 'crm_metric_scope', METRIC],
  ])('the current-scope head folds into the owner select-body plan for %#', (assignment, scope, alias, tail) => {
    expect(crmCurrentScopeAllowedSql(assignment, scope, alias, tail)).toBe(
      crmSelectBodySql(`${assignment}${tail} allowed FROM ${scope} ${alias}`),
    );
    expect(crmCurrentScopeAllowedSql(assignment, scope, alias, tail)).toBe(
      `SELECT ${assignment}${tail} allowed FROM ${scope} ${alias}`,
    );
  });

  test('retirement:ts-statements folds the current-scope head (30 -> 29)', () => {
    expect(totalSelects()).toBe(23);
    expect(totalSelects()).toBeLessThan(30);
  });
});
