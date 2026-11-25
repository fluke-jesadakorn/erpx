import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import {
  selectOneFromWhereSql,
  releaseTableExistsSql,
  derivedSourceExistsSql,
  latestReleaseIdSql,
  releaseAcceptanceRowSql,
  releaseRevisionTable,
} from '../../../packages/server-platform_administration/src/adapters/release-predicate';
import { productionScope } from '@erpx/catalogue/production-scope';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const rev = releaseRevisionTable(productionScope.version);

// Pre-fold renderings captured verbatim from the BEFORE=23 owner source. They
// are byte-identity oracles only: none of these literal heads remains inline.
const preConfiguration = `EXISTS(SELECT 1 FROM ${rev} WHERE company_id=? AND revision=?)`;
const preDerived = 'SELECT 1 FROM (SRC) a WHERE a.id=?';
const preLatest = '(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=? AND latest.scope_version=? ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1)';
const preLatestQualified = '(SELECT latest.id FROM company_release_acceptances latest WHERE latest.company_id=release.company_id AND latest.scope_version=release.scope_version ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1)';

describe('Platform Administration EXISTS-head and latest-id parameterized fold', () => {
  test('one owner-local one-row head renders byte-identically for both EXISTS families', () => {
    expect(selectOneFromWhereSql('t', 'x=1')).toBe('SELECT 1 FROM t WHERE x=1');
    expect(selectOneFromWhereSql('t', 'x=1', '\n  ')).toBe('SELECT 1 FROM t\n  WHERE x=1');
    expect(releaseTableExistsSql(rev, 'company_id=? AND revision=?')).toBe(preConfiguration);
    expect(releaseTableExistsSql('t', 'x=1', '\n  ')).toBe('EXISTS(SELECT 1 FROM t\n  WHERE x=1)');
    expect(derivedSourceExistsSql('SRC', 'a', 'a.id=?')).toBe(preDerived);
    expect(releaseTableExistsSql('t', 'x=1')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(derivedSourceExistsSql('SRC', 'a', 'a.id=?')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the latest-acceptance id subquery renders byte-identically through the row head', () => {
    expect(latestReleaseIdSql('?', '?')).toBe(preLatest);
    expect(latestReleaseIdSql('release.company_id', 'release.scope_version')).toBe(preLatestQualified);
    expect(releaseAcceptanceRowSql(
      'latest.id',
      'latest.company_id=? AND latest.scope_version=?',
      ' ORDER BY latest.created_at DESC,latest.id DESC LIMIT 1',
      'company_release_acceptances latest',
    )).toBe(preLatest.slice(1, -1));
    expect(latestReleaseIdSql('?', '?')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the two former heads each survive once and both callers compose them', () => {
    const predicate = source('adapters/release-predicate.ts');
    expect(predicate.match(/export function selectOneFromWhereSql/g)).toHaveLength(1);
    expect(predicate.split('`SELECT 1 FROM ${from}${gap}WHERE ${where}`').length - 1).toBe(1);
    expect(predicate).toContain('return `EXISTS(${selectOneFromWhereSql(from, where, gap)})`;');
    expect(predicate).toContain('return selectOneFromWhereSql(`(${source}) ${alias}`, predicate);');
    expect(predicate).not.toContain('SELECT 1 FROM (${source}) ${alias} WHERE ${predicate}');
    expect(predicate.split("releaseAcceptanceRowSql('latest.id',").length - 1).toBe(1);
    expect(predicate).not.toContain('SELECT latest.id FROM company_release_acceptances latest WHERE');
  });

  test('locks, native reads and earlier families are untouched', () => {
    const predicate = source('adapters/release-predicate.ts');
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain("releaseAcceptanceRowSql('company_id','company_id=?',' FOR SHARE','engineering_verification_contexts')");
    expect(source('adapters/engineering-verification.ts')).toContain('/*erpx-native-engineering-fence*/');
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('readNativeReleaseChildImage(actor,id)');
    expect(predicate).toContain('export function releaseAcceptanceRowSql');
    expect(predicate).toContain('export function projectionSourceHeadSql');
    expect(predicate).toContain('export function releaseCurrencyPredicate');
  });

  test('retirement:ts-statements drops below the 23 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(18);
    expect(total).toBeLessThan(23);
  });
});
