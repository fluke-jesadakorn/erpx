import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { activeIamRoleProbeSql } from '../../../packages/server-identity_access/src/adapters/active-role-probe-family-r25';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const helperSrc = read('adapters/active-role-probe-family-r25.ts');
const techSrc = read('adapters/technology.ts');
const peopleSrc = read('adapters/people-template-attributes.ts');

// Both pre-dedupe literals shared this exact head; only the fixed
// published-revision suffix differed. The two originals are reconstructed
// verbatim here so byte-identical rendering is asserted, not merely shape.
const head = "SELECT 1 FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'";
const techBefore = `${head} AND published_revision=$3`;
const peopleBefore = head;

describe('Identity access read-tail r25 active role probe family', () => {
  test('renders byte-identical SQL to both pre-dedupe literals', () => {
    expect(activeIamRoleProbeSql(true)).toBe(techBefore);
    expect(activeIamRoleProbeSql(false)).toBe(peopleBefore);
  });

  test('is one owner helper with one live call site after lane 12 (r25 had two)', () => {
    expect((helperSrc.match(/export const activeIamRoleProbeSql =/g) ?? []).length).toBe(1);
    expect((helperSrc.match(/\bactiveIamRoleProbeSql\b/g) ?? []).length).toBe(1); // definition only
    expect((techSrc.match(/\bactiveIamRoleProbeSql\b/g) ?? []).length).toBe(2); // import + call
    // Lane 12 moved the people-template active-role read to the native owner.
    expect((peopleSrc.match(/\bactiveIamRoleProbeSql\b/g) ?? []).length).toBe(0);
    expect(peopleSrc).toContain("from './native-lane-12/client'");
    const headMarker = "FROM iam_roles WHERE id=$1 AND company_id IN ('*',$2) AND status='active'";
    expect((techSrc.match(new RegExp(headMarker.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g')) ?? []).length).toBe(0);
    expect((peopleSrc.match(new RegExp(headMarker.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g')) ?? []).length).toBe(0);
  });

  test('introduces no write or lock and leaves locks/session authority/native kinds/earlier helpers untouched', () => {
    expect(helperSrc).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // lock families remain exactly where they were
    expect(read('adapters/payroll-cancellation-grant.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE');
    // earlier-round helpers still present exactly once
    const probeSql = read('adapters/iam-probe-sql.ts');
    for (const h of [
      'projectAuthorityProbeSql', 'roleAllowsSql', 'samePersonSql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'grantProbeSql', 'publishedRoleSql',
    ]) expect((probeSql.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length, h).toBe(1);
    expect((read('adapters/identity-user-projection-family-r18.ts').match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
    expect((read('adapters/identity-user-projection-family-r21.ts').match(/export const userAttributeByIdExtendedSql =/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r25 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r25 BEFORE=518 baseline (expected 517)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(518);
    expect(total).toBeLessThanOrEqual(517);
  });
});
