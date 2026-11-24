import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { overrideAuthorityProbe } from '../../../packages/server-identity_access/src/adapters/workflow-approvals-facts';
import { identityVerifiedContextProjections as organizationFinalOwnerProjections } from '@erpx/server-organization/public/identity-access-verified-context-projections';
import { identitySnapshotSources as peoplepayrollSnapshotSources } from '@erpx/server-people_payroll/public/identity-snapshot-sources';

const organizationFinalSources = organizationFinalOwnerProjections();
const peoplepayrollSources = peoplepayrollSnapshotSources();

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const factsSrc = read('adapters/workflow-approvals-facts.ts');

// The two pre-dedupe literals shared this entire head/tail; only the engagement
// join fragment differed. Both are reconstructed here from the same owner source
// fragments so byte-identical rendering is asserted, not merely structural shape.
const head = `EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (iam_role_allows(a.role,a.company_id,'workflow.people_manager') AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?) OR EXISTS(SELECT 1 ${organizationFinalSources.workflow_approvals_facts_authority_seats_s_from} ${peoplepayrollSources.workflow_approvals_facts_hr_people_p_join} ON p.id=s.person_id AND p.company_id=s.company_id `;
const tail = `WHERE s.company_id=a.company_id AND s.kind IN ('ceo','hr') AND iam_same_person(p.user_id,a.user_id,a.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint)))`;
const engagementJoin = `${peoplepayrollSources.workflow_approvals_facts_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active' `;

describe('Identity access read-tail r24 override authority probe family', () => {
  test('renders byte-identical SQL to both pre-dedupe literals', () => {
    expect(overrideAuthorityProbe(engagementJoin)).toBe(`${head}${engagementJoin}${tail}`);
    expect(overrideAuthorityProbe('')).toBe(`${head}${tail}`);
  });

  test('is one owner helper with two call sites and no inline literal left behind', () => {
    expect((factsSrc.match(/export const overrideAuthorityProbe/g) ?? []).length).toBe(1);
    expect((factsSrc.match(/\boverrideAuthorityProbe\b/g) ?? []).length).toBe(3); // def + two calls
    const headMarker = `EXISTS(SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=?`;
    expect((factsSrc.match(new RegExp(headMarker.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'g')) ?? []).length).toBe(1); // only in helper
  });

  test('introduces no write or lock and leaves locks/session authority/native kinds/earlier helpers untouched', () => {
    const newHelper = factsSrc.slice(factsSrc.indexOf('export const overrideAuthorityProbe'));
    expect(newHelper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // lock families remain exactly where they were
    expect(read('adapters/payroll-cancellation-grant.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE');
    // earlier-round helpers still present exactly once
    const probeSql = read('adapters/iam-probe-sql.ts');
    for (const h of [
      'projectAuthorityProbeSql', 'roleAllowsSql', 'samePersonSql', 'activeRecipientsSql',
      'idNameByUserIdsSql', 'verifiedUserEmailSql', 'grantProbeSql',
    ]) expect((probeSql.match(new RegExp(`export const ${h} =`, 'g')) ?? []).length, h).toBe(1);
    expect((read('adapters/identity-user-projection-family-r18.ts').match(/export const userAttributeByIdSql =/g) ?? []).length).toBe(1);
    expect((read('adapters/identity-user-projection-family-r21.ts').match(/export const userAttributeByIdExtendedSql =/g) ?? []).length).toBe(1);
  });
});

describe('Identity access read-tail r24 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r24 BEFORE=520 baseline (expected 518)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(520);
    expect(total).toBeLessThanOrEqual(518);
  });
});
