import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { derivedSourceExistsSql } from '../../../packages/server-platform_administration/src/adapters/release-predicate';
import { platformOwnerProjectionSources as identityAccessSources } from '@erpx/server-identity_access/public/platform-owner-projection-sources';
import { productionScope, criteriaForScope } from '@erpx/catalogue/production-scope';

const srcRoot = resolve(__dirname, '../../../packages/server-platform_administration/src');
const source = (relative: string) => readFileSync(resolve(srcRoot, relative), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

const assignments = identityAccessSources.assignments;
const criteria = criteriaForScope(productionScope.version)
  .map(row => `('${row.id.replaceAll("'", "''")}','${row.permission.replaceAll("'", "''")}')`).join(',');

// Byte oracles captured from the BEFORE=26 source. Each rendered head is
// byte-identical to the former inline copy; neither remains inline after r20.
const pre = {
  reviews: `SELECT 1 FROM (${assignments}) a WHERE a.id=g.assignment_id
    AND a.company_id=g.company_id AND a.user_id=g.reviewer_id AND a.scope_kind='holding'
    AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(a.role,a.company_id,g.permission)
    AND NOT iam_same_person(g.reviewer_id,?,g.company_id)`,
  required: `SELECT 1 FROM (VALUES ${criteria}) AS required(id,permission) WHERE NOT EXISTS(
      SELECT 1 FROM company_release_gate_reviews gate JOIN (${assignments}) reviewer ON reviewer.id=gate.assignment_id
      WHERE gate.company_id=release.company_id AND gate.release_id=release.id AND gate.criterion_id=required.id
      AND gate.state='passed' AND gate.permission=required.permission AND reviewer.user_id=gate.reviewer_id
      AND reviewer.company_id=release.company_id AND reviewer.scope_kind='holding'
      AND iam_assignment_active(reviewer.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(reviewer.user_id,reviewer.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(reviewer.role,reviewer.company_id,required.permission)
      AND NOT iam_same_person(gate.reviewer_id,release.requested_by,release.company_id))`,
};

const preSha: Record<keyof typeof pre, string> = {
  reviews: '9cf3c9e88b473a626d2dc0e810653435e335a1a8da41218709b1cf80accd44d1',
  required: '2bbc2864a5929308069892b4ad887c60f5fac0ba9708d65267bd0da79e2986c4',
};

describe('Platform Administration r20 derived-source EXISTS head fold', () => {
  test('both derived-source heads render byte-identically through one parameterized head', () => {
    const after = {
      reviews: derivedSourceExistsSql(assignments, 'a', `a.id=g.assignment_id
    AND a.company_id=g.company_id AND a.user_id=g.reviewer_id AND a.scope_kind='holding'
    AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(a.role,a.company_id,g.permission)
    AND NOT iam_same_person(g.reviewer_id,?,g.company_id)`),
      required: derivedSourceExistsSql(`VALUES ${criteria}`, 'AS required(id,permission)', `NOT EXISTS(
      SELECT 1 FROM company_release_gate_reviews gate JOIN (${assignments}) reviewer ON reviewer.id=gate.assignment_id
      WHERE gate.company_id=release.company_id AND gate.release_id=release.id AND gate.criterion_id=required.id
      AND gate.state='passed' AND gate.permission=required.permission AND reviewer.user_id=gate.reviewer_id
      AND reviewer.company_id=release.company_id AND reviewer.scope_kind='holding'
      AND iam_assignment_active(reviewer.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(reviewer.user_id,reviewer.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(reviewer.role,reviewer.company_id,required.permission)
      AND NOT iam_same_person(gate.reviewer_id,release.requested_by,release.company_id))`),
    };
    const headSelects: Record<keyof typeof pre, number> = { reviews: 2, required: 3 };
    for (const key of Object.keys(pre) as Array<keyof typeof pre>) {
      expect(after[key]).toBe(pre[key]);
      expect(sha(after[key])).toBe(preSha[key]);
      expect(after[key].split('SELECT ').length - 1).toBe(headSelects[key]);
      expect(after[key]).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(after.reviews.match(/\?/g)).toHaveLength(1);
    expect(after.required.match(/\?/g) ?? []).toHaveLength(0);
  });

  test('the two former heads are one owner-local head with two composed callers', () => {
    const predicate = source('adapters/release-predicate.ts');
    const acceptance = source('adapters/release-acceptance.ts');
    expect(predicate.match(/export function derivedSourceExistsSql/g)).toHaveLength(1);
    expect(predicate.split('`SELECT 1 FROM (${source}) ${alias} WHERE ${predicate}`').length - 1).toBe(1);
    expect(predicate).toContain('derivedSourceExistsSql(`VALUES ${criteria}`,\'AS required(id,permission)\',`NOT EXISTS(');
    expect(acceptance).toContain("derivedSourceExistsSql(identity_accessPlatformSources.assignments,'a',`a.id=g.assignment_id");
    expect(predicate).not.toContain('NOT EXISTS(SELECT 1 FROM (VALUES ${criteria})');
    expect(acceptance).not.toContain('EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.assignments}) a WHERE');
  });

  test('locks, native writes, the engineering fence and prior families are untouched', () => {
    expect(source('adapters/release-acceptance.ts')).toContain("export const RELEASE_ROW_LOCK_SQL = 'SELECT id FROM company_release_acceptances WHERE company_id=? AND id=? FOR UPDATE'");
    expect(source('adapters/pms-rollout-guards.ts')).toContain('SELECT id FROM company_release_acceptances WHERE id=? FOR SHARE');
    expect(source('adapters/collaboration-reply-rollout.ts')).toContain('SELECT company_id FROM engineering_verification_contexts WHERE company_id=? FOR SHARE');
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('LOCK TABLE company_release_acceptances,company_release_gate_reviews,company_release_events,company_release_revisions IN SHARE ROW EXCLUSIVE MODE');
    expect(source('adapters/engineering-verification.ts')).toContain('/*erpx-native-engineering-fence*/');
    expect(source('adapters/engineering-verification.ts')).toContain('pg_catalog.pg_database');
    expect(source('adapters/native-owner-write.ts')).toContain('export const NATIVE_OWNER_WRITE_NOOP_SQL');
    expect(source('adapters/native-owner-write.ts')).toContain('export function selectOneSql');
    expect(source('adapters/release-predicate.ts')).toContain('iam_assignment_active(reviewer.id');
    expect(source('adapters/release-predicate.ts')).toContain('organizationPlatformSources.company_business_acceptance');
    expect(source('adapters/release-predicate.ts')).toContain('export function releaseTableExistsSql');
    expect(source('adapters/release-predicate.ts')).toContain('export function projectionSourceHeadSql');
    expect(source('adapters/release-predicate.ts')).toContain('export function releaseAcceptanceRowSql');
    expect(source('adapters/company-release-attachment-read-sources.ts')).toContain('readNativeReleaseChildImage(actor,id)');
  });
});

describe('Platform Administration r20 retirement check', () => {
  test('retirement:ts-statements is pinned at the post-r20 25 baseline (BEFORE=26)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(23);
    expect(total).toBeLessThan(26);
  });
});
