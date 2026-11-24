import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { latestReleaseRowSql } from '../../../packages/server-platform_administration/src/adapters/release-predicate';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');

// Pre-dedupe literals copied verbatim from HEAD (r13 baseline, BEFORE=35). Used
// only as byte-identity oracles; the two inline row reads are no longer present.
const preRow = (idPredicate: string) => `SELECT * FROM company_release_acceptances WHERE company_id=? ${idPredicate} ORDER BY created_at DESC,id DESC LIMIT 1`;
const preLatest = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND scope_version=? ORDER BY created_at DESC,id DESC LIMIT 1';

describe('Platform Administration latest-release row parameterized-family dedupe (r13)', () => {
  test('the shared row read renders byte-identical SQL and stays read-only', () => {
    expect(latestReleaseRowSql('*', 'AND id=?')).toBe(preRow('AND id=?'));
    expect(latestReleaseRowSql('*', 'AND scope_version=?')).toBe(preRow('AND scope_version=?'));
    expect(latestReleaseRowSql('id', 'AND scope_version=?')).toBe(preLatest);
    expect(latestReleaseRowSql('*', 'AND id=?')).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the row-read head survives once and both repository lookups compose it', () => {
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    const head = 'FROM company_release_acceptances WHERE ${predicate}${suffix}';
    expect(predicate.match(/export function releaseAcceptanceRowSql/g)).toHaveLength(1);
    expect(predicate.match(/export function latestReleaseRowSql/g)).toHaveLength(1);
    expect(predicate.split(head).length - 1).toBe(1);
    expect(predicate).toContain("releaseAcceptanceRowSql(columns, `company_id=? ${idPredicate}`, ' ORDER BY created_at DESC,id DESC LIMIT 1')");
    expect(acceptance).toContain("latestReleaseRowSql('*', releaseId?'AND id=?':'AND scope_version=?')");
    expect(acceptance).toContain("latestReleaseRowSql('id','AND scope_version=?')");
    expect(acceptance).not.toContain(preRow('AND id=?'));
    expect(acceptance).not.toContain(preRow('AND scope_version=?'));
    expect(acceptance).not.toContain(preLatest);
  });

  test('locks, the r1-r12 helpers and the latestReleaseId scalar subquery are untouched', () => {
    const predicate = source('adapters/release-predicate.ts');
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    for (const helper of ['thbCurrencyProbeSql', 'releaseAcceptanceExistsSql', 'latestReleaseIdSql', 'activeBranchCountSql', 'businessAcceptanceSetupSql', 'releaseRevisionReadSql', 'releaseRevisionShareLockSql']) {
      expect(predicate).toContain(`export function ${helper}`);
    }
  });

  test('retirement:ts-statements stays strictly below the r13 BEFORE=35 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(35);
  });
});
