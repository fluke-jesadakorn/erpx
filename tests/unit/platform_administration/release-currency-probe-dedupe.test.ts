import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { releaseCurrencyPredicate, thbCurrencyProbeSql } from '../../../packages/server-platform_administration/src/adapters/release-predicate';
import { platformOwnerProjectionSources as financePlatformSources } from '../../../packages/server-finance/src/public/platform-owner-projection-sources';
import { platformOwnerProjectionSources as commercePlatformSources } from '../../../packages/server-commerce/src/public/platform-owner-projection-sources';
import { platformOwnerProjectionSources as posPlatformSources } from '../../../packages/server-pos/src/public/platform-owner-projection-sources';
import { platformOwnerProjectionSources as pmsPlatformSources } from '../../../packages/server-pms/src/public/platform-owner-projection-sources';
import { platformOwnerProjectionSources as people_payrollPlatformSources } from '../../../packages/server-people_payroll/src/public/platform-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');

// Pre-dedupe literal copied verbatim from HEAD (r10 baseline, BEFORE=42). Used
// only as a byte-identity oracle; the five inline probes are no longer present.
const preCurrencyPredicate = (companyExpression: string) => `(SELECT NOT EXISTS(SELECT 1 FROM (${financePlatformSources.finance_charts}) c WHERE c.company_id=scope_company.id AND c.base_currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_settings}) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${posPlatformSources.pos_settings}) c WHERE c.company_id=scope_company.id AND c.currency<>'THB')
    AND NOT EXISTS(SELECT 1 FROM (${pmsPlatformSources.pms_properties}) c WHERE c.company_id=scope_company.id AND c.config_json::jsonb->>'currency' IS DISTINCT FROM 'THB')
    AND NOT EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_pay_groups}) c WHERE c.company_id=scope_company.id AND c.status='active' AND (c.country_code<>'TH' OR c.currency<>'THB'))
    FROM (SELECT ${companyExpression}::text AS id) scope_company)`;

describe('Platform Administration release-currency parameterized-family dedupe (r10)', () => {
  test('the shared probe renders byte-identical SQL and stays read-only', () => {
    expect(thbCurrencyProbeSql('SRC', "c.base_currency<>'THB'"))
      .toBe("SELECT 1 FROM (SRC) c WHERE c.company_id=scope_company.id AND c.base_currency<>'THB'");
    expect(thbCurrencyProbeSql('SRC', 'c.x=1')).toContain('SELECT ');
    expect(thbCurrencyProbeSql('SRC', 'c.x=1')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('releaseCurrencyPredicate renders byte-identical to the pre-dedupe literal', () => {
    for (const expression of ['?', 'release.company_id']) {
      expect(releaseCurrencyPredicate(expression)).toBe(preCurrencyPredicate(expression));
    }
  });

  test('the probe body survives once in the helper and every family caller is composed', () => {
    const text = source('adapters/release-predicate.ts');
    expect(text.match(/export function thbCurrencyProbeSql/g)).toHaveLength(1);
    expect(text.split('c.company_id=scope_company.id').length - 1).toBe(1);
    for (const call of [
      'thbCurrencyProbeSql(financePlatformSources.finance_charts,',
      'thbCurrencyProbeSql(commercePlatformSources.commerce_settings,',
      'thbCurrencyProbeSql(posPlatformSources.pos_settings,',
      'thbCurrencyProbeSql(pmsPlatformSources.pms_properties,',
      'thbCurrencyProbeSql(people_payrollPlatformSources.hr_pay_groups,',
    ]) expect(text).toContain(call);
    for (const inline of [
      'NOT EXISTS(SELECT 1 FROM (${financePlatformSources.finance_charts})',
      'NOT EXISTS(SELECT 1 FROM (${people_payrollPlatformSources.hr_pay_groups})',
    ]) expect(text).not.toContain(inline);
  });
});

describe('Platform Administration r10 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r10 BEFORE=42 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(42);
  });
});
