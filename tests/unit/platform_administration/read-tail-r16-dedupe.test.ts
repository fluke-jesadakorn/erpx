import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import {
  releaseAcceptanceExistsSql,
  releaseTableExistsSql,
  activeBranchCountSql,
  businessAcceptanceSetupSql,
  thbCurrencyProbeSql,
  projectionSourceHeadSql,
  latestReleaseIdSql,
  releaseCurrencyPredicate,
} from '../../../packages/server-platform_administration/src/adapters/release-predicate';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');

// Byte oracles captured from the BEFORE=31 source of this task. They are
// rendered output only: no inline copy of these heads remains in the owner.
const preVersion = 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)';
const preLatest = 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND id=(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=? AND latest.scope_version=? ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1))';
const preAttachment = 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE id=? AND company_id=? AND version=?)';

const preBranchDefault = 'SELECT count(*) FROM (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes) organization_nodes WHERE company_id=? AND kind=\'branch\' AND status=\'active\'';
const preBranchCountAlias = 'SELECT count(*) count FROM (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes) organization_nodes WHERE company_id=? AND kind=\'branch\' AND status=\'active\'';
const preBranchQualified = 'SELECT count(*) FROM (SELECT "id","kind","slug","display_name","parent_id","company_id","branch_id","status","hierarchy_revision" FROM organization_nodes) branch WHERE branch.company_id=release.company_id AND branch.kind=\'branch\' AND branch.status=\'active\'';
const preSetupGuard = 'SELECT 1 FROM (SELECT "company_id","status","version" FROM company_business_acceptance) company_business_acceptance WHERE company_id=? AND status=\'accepted\' AND version=?';
const preSetupInputs = 'SELECT version,status FROM (SELECT "company_id","status","version" FROM company_business_acceptance) company_business_acceptance WHERE company_id=?';
const preThb = "SELECT 1 FROM (SRC) c WHERE c.company_id=scope_company.id AND c.base_currency<>'THB'";
const preThbX = 'SELECT 1 FROM (SRC) c WHERE c.company_id=scope_company.id AND c.x=1';
const preCurrency = `(SELECT NOT EXISTS(SELECT 1 FROM (SELECT "id","company_id","name","base_currency","accounting_basis","status","version","created_by","created_at","updated_at" FROM finance_charts) c WHERE c.company_id=scope_company.id AND c.base_currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (SELECT "company_id","currency" FROM pos_settings) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (SELECT "id","company_id","branch_id","name","business_date","state","config_json","version","created_by","updated_at" FROM pms_properties) c WHERE c.company_id=scope_company.id AND c.config_json::jsonb->>'currency' IS DISTINCT FROM 'THB')
    AND NOT EXISTS(SELECT 1 FROM (SELECT "id","company_id","name","country_code","currency","frequency","timezone","calendar_json","status","created_by","created_at","updated_at","overtime_multiplier_bps","proration_method","schedule_enabled","pay_delay_days","schedule_anchor_date","schedule_version" FROM hr_pay_groups) c WHERE c.company_id=scope_company.id AND c.status='active' AND (c.country_code<>'TH' OR c.currency<>'THB'))
    FROM (SELECT ?::text AS id) scope_company)`;

describe('Platform Administration read-tail r16 parameterized-family dedupe', () => {
  test('F1: the acceptance-exists wrapper renders byte-identically through the fixed-table helper', () => {
    expect(releaseAcceptanceExistsSql('company_id=? AND id=? AND version=? AND revoked_at IS NULL')).toBe(preVersion);
    expect(releaseAcceptanceExistsSql(`company_id=? AND id=? AND id=${latestReleaseIdSql('?', '?')}`)).toBe(preLatest);
    expect(releaseAcceptanceExistsSql('id=? AND company_id=? AND version=?')).toBe(preAttachment);
    expect(releaseAcceptanceExistsSql('x=1')).toBe(releaseTableExistsSql('company_release_acceptances', 'x=1'));
    expect(releaseAcceptanceExistsSql('x=1')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);

    const predicate = source('adapters/release-predicate.ts');
    expect(predicate).toContain("releaseTableExistsSql('company_release_acceptances', where)");
    expect(predicate.split('EXISTS(SELECT 1 FROM company_release_acceptances WHERE ').length - 1).toBe(0);
    expect(predicate.match(/export function releaseAcceptanceExistsSql/g)).toHaveLength(1);
    expect(predicate.match(/export function releaseTableExistsSql/g)).toHaveLength(1);
  });

  test('F2: one closed projection-source head renders byte-identically for all three callers', () => {
    expect(projectionSourceHeadSql('1', 'SRC', 'c', 'c.x=1')).toBe('SELECT 1 FROM (SRC) c WHERE c.x=1');
    expect(activeBranchCountSql('?')).toBe(preBranchDefault);
    expect(activeBranchCountSql('?', { countAlias: 'count' })).toBe(preBranchCountAlias);
    expect(activeBranchCountSql('release.company_id', { qualifier: 'branch' })).toBe(preBranchQualified);
    expect(businessAcceptanceSetupSql('?', { acceptedVersion: '?' })).toBe(preSetupGuard);
    expect(businessAcceptanceSetupSql('?', { columns: 'version,status' })).toBe(preSetupInputs);
    expect(thbCurrencyProbeSql('SRC', "c.base_currency<>'THB'")).toBe(preThb);
    expect(thbCurrencyProbeSql('SRC', 'c.x=1')).toBe(preThbX);
    expect(releaseCurrencyPredicate('?')).toBe(preCurrency);
    for (const sql of [preBranchDefault, preSetupGuard, preThb]) {
      expect(sql).toMatch(/^SELECT .+ FROM \(.+\) \w+ WHERE /);
      expect(sql).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }

    const predicate = source('adapters/release-predicate.ts');
    expect(predicate.match(/export function projectionSourceHeadSql/g)).toHaveLength(1);
    expect(predicate.split('`SELECT ${columns} FROM (${projection}) ${alias} WHERE ${predicate}`').length - 1).toBe(1);
    expect(predicate).toContain('projectionSourceHeadSql(count, organizationPlatformSources.organization_nodes,');
    expect(predicate).toContain("projectionSourceHeadSql(columns, organizationPlatformSources.company_business_acceptance, 'company_business_acceptance'");
    expect(predicate).toContain("projectionSourceHeadSql('1', source, 'c', `c.company_id=scope_company.id AND ${condition}`)");
    expect(predicate.split("organization_nodes}) ${options.qualifier ?? 'organization_nodes'} WHERE").length - 1).toBe(0);
    expect(predicate.split('FROM (${organizationPlatformSources.company_business_acceptance}) company_business_acceptance WHERE company_id=').length - 1).toBe(0);
    expect(predicate.split('SELECT 1 FROM (${source}) c WHERE c.company_id=scope_company.id').length - 1).toBe(0);
    for (const helper of ['activeBranchCountSql', 'businessAcceptanceSetupSql', 'thbCurrencyProbeSql']) {
      expect(predicate.match(new RegExp(`export function ${helper}`, 'g'))).toHaveLength(1);
    }
  });

  test('F1 bind arrays keep their exact order', () => {
    const acceptance = source('adapters/release-acceptance.ts');
    const attachment = source('adapters/attachments-finance-facts.ts');
    expect(acceptance.match(/\[actor\.companyId,\s*id,\s*input\.expectedVersion\]/g)).toHaveLength(3);
    expect(acceptance).toContain('[actor.companyId, id, actor.companyId, scope]');
    expect(attachment).toContain('[targetId,companyId,version]');
  });

  test('locks, pg_catalog fence, native-write placeholders and records-ai fragment are untouched', () => {
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain('SELECT company_id FROM engineering_verification_contexts WHERE company_id=? FOR SHARE');
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(source('adapters/engineering-verification.ts')).toContain('/*erpx-native-engineering-fence*/');
    expect(source('adapters/engineering-verification.ts')).toContain('pg_catalog.pg_database');
    expect(source('adapters/native-owner-write.ts')).toContain('export const NATIVE_OWNER_WRITE_NOOP_SQL');
    expect(source('adapters/native-owner-write.ts')).toContain('export function selectOneSql');
    expect(source('adapters/records-ai-usage-projection.ts')).toContain('export const recordsExecutiveUsageSql');
  });
});

describe('Platform Administration r16 retirement check', () => {
  test('retirement:ts-statements is pinned at the post-r19 26 baseline (BEFORE=28)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(26);
    expect(total).toBeLessThan(31);
  });
});
