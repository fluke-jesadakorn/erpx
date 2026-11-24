import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { releaseAcceptanceExistsSql, latestReleaseIdSql, thbCurrencyProbeSql } from '../../../packages/server-platform_administration/src/adapters/release-predicate';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');

// Pre-dedupe literals copied verbatim from HEAD (r11 baseline, BEFORE=38). Used
// only as byte-identity oracles; the three inline guards are no longer present.
const preVersionGuard = 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND version=? AND revoked_at IS NULL)';
const preLatestGuard = `EXISTS(SELECT 1 FROM company_release_acceptances WHERE company_id=? AND id=? AND id=${latestReleaseIdSql('?', '?')})`;
const preAttachmentGuard = 'EXISTS(SELECT 1 FROM company_release_acceptances WHERE id=? AND company_id=? AND version=?)';

describe('Platform Administration release-acceptance existence-guard parameterized-family dedupe (r11)', () => {
  test('the shared guard renders byte-identical SQL and stays read-only', () => {
    expect(releaseAcceptanceExistsSql('company_id=? AND id=? AND version=? AND revoked_at IS NULL')).toBe(preVersionGuard);
    expect(releaseAcceptanceExistsSql(`company_id=? AND id=? AND id=${latestReleaseIdSql('?', '?')}`)).toBe(preLatestGuard);
    expect(releaseAcceptanceExistsSql('id=? AND company_id=? AND version=?')).toBe(preAttachmentGuard);
    expect(releaseAcceptanceExistsSql('x=1')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the guard body survives once in the helper and every family caller is composed', () => {
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    const attachment = source('adapters/attachments-finance-facts.ts');
    const all = `${predicate}\n${acceptance}\n${attachment}`;
    expect(predicate.match(/export function releaseAcceptanceExistsSql/g)).toHaveLength(1);
    // The guard head now lives once in the shared fixed-table helper; the
    // wrapper delegates and no longer inlines the literal.
    expect(predicate).toContain("releaseTableExistsSql('company_release_acceptances', where)");
    expect(all.split('EXISTS(SELECT 1 FROM company_release_acceptances WHERE ').length - 1).toBe(0);
    expect(acceptance).toContain("releaseAcceptanceExistsSql('company_id=? AND id=? AND version=? AND revoked_at IS NULL')");
    expect(acceptance).toContain('releaseAcceptanceExistsSql(`company_id=? AND id=? AND id=${latestReleaseIdSql(\'?\', \'?\')}`)');
    expect(attachment).toContain('releaseAcceptanceExistsSql("id=? AND company_id=? AND version=?")');
    for (const inline of [preVersionGuard, preLatestGuard, preAttachmentGuard]) {
      expect(acceptance).not.toContain(`'${inline}'`);
      expect(attachment).not.toContain(`"${inline}"`);
    }
  });

  test('locks and the r10 currency-probe helper are untouched', () => {
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/release-predicate.ts')).toContain('export function thbCurrencyProbeSql');
    expect(thbCurrencyProbeSql('SRC', 'c.x=1')).toBe('SELECT 1 FROM (SRC) c WHERE c.company_id=scope_company.id AND c.x=1');
  });
});

describe('Platform Administration r11 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r11 BEFORE=38 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(38);
  });
});
