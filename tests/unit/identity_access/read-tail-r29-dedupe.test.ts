import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { identitySnapshotSources as peoplepayrollSnapshotSources } from '@erpx/server-people_payroll/public/identity-snapshot-sources';
import { identitySnapshotSources as organizationSnapshotSources } from '@erpx/server-organization/public/identity-snapshot-sources';
import {
  assignmentActiveProbeSql, assignmentUserScopeProbeSql, distinctAssigneeSql,
} from '../../../packages/server-identity_access/src/adapters/read-tail-family-r29';

const peoplepayrollSources = peoplepayrollSnapshotSources();
const organizationSources = organizationSnapshotSources();
const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Pre-dedupe literals captured from HEAD. Every rendered helper output must match exactly.
const O_F1_LEGAL_ACTOR = `SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?
    AND (?::bigint IS NULL OR version=?)
    AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)
    AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)
    AND iam_role_allows(role,company_id,'legal.ai')`;
const O_F1_LEGAL_LEASE = `SELECT 1 FROM assignments WHERE id=? AND user_id=? AND revoked_at IS NULL AND valid_until>? AND valid_from<=?`;
const O_F1_LEGAL_AFTERGEN = `SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?`;
const O_F1_WF_COMMAND = `EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND role=? AND company_id=? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?::text) OR (scope_kind='department' AND scope_node_id=?::text)) AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`;
const O_F1_RECORDS_AI = `EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?)`;
const O_F1_COLLAB_AGENT = `EXISTS(SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?)`;
const O_F2_WF_GUARD26 = `EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND
    (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))))`;
const O_F1_WF_APPROVAL_SCOPE = `SELECT 1 FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (? OR scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)`;
const O_F1_BETA_ASSIGNMENT = `SELECT 1 FROM assignments WHERE id=? AND user_id=? AND revoked_at IS NULL AND valid_until>?`;
const O_F2_BETA_INVITER = `EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND iam_role_allows(role,company_id,'company.owner') AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?)`;
const O_F2_PMS_ASSIGNEE = `SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?))`;
const O_F2_PEOPLE_DIR_GUARD7 = `EXISTS(SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?) AND NOT EXISTS(SELECT 1 ${peoplepayrollSources.people_directory_facts_hr_people_from} WHERE user_id=? AND company_id=? AND id!=? AND merged_into_id IS NULL)`;
const O_F3_OPS_ELIGIBLE = `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'workflow.'||?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?
    AND (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))) ORDER BY user_id`;
const O_F2_OPS_ROUTE = `SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND iam_role_allows(role,company_id,'workflow.people_manager') AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?) AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)`;
const O_F3_RECORDS_EMPLOYEE = `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND iam_role_allows(role,company_id,'company.owner') AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;
const O_F3_COLLAB_TEXT = `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?`;
const O_F3_MEMBER_DIR = `SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IS NOT NULL AND ((revoked_at IS NULL AND valid_from<=? AND valid_until>?) OR ?=1) AND (?::text IS NULL OR scope_kind='holding' OR COALESCE(branch_id,(SELECT branch_id ${organizationSources.member_directory_organization_nodes_from} WHERE id=scope_node_id))=?) ORDER BY user_id`;

describe('Identity access read-tail r29 assignment probe families', () => {
  test('all 17 rendered heads are byte-identical to their pre-dedupe literals', () => {
    expect(assignmentActiveProbeSql(`company_id=? AND role=?
    AND (?::bigint IS NULL OR version=?)
    AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)
    AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)
    AND iam_role_allows(role,company_id,'legal.ai')`)).toBe(O_F1_LEGAL_ACTOR);
    expect(assignmentActiveProbeSql(`revoked_at IS NULL AND valid_until>? AND valid_from<=?`)).toBe(O_F1_LEGAL_LEASE);
    expect(assignmentActiveProbeSql(`company_id=? AND role=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?`)).toBe(O_F1_LEGAL_AFTERGEN);
    expect(`EXISTS(${assignmentActiveProbeSql(`role=? AND company_id=? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?::text) OR (scope_kind='department' AND scope_node_id=?::text)) AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`)})`).toBe(O_F1_WF_COMMAND);
    expect(`EXISTS(${assignmentActiveProbeSql(`role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?`)})`).toBe(O_F1_RECORDS_AI);
    expect(`EXISTS(${assignmentActiveProbeSql(`company_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?`)})`).toBe(O_F1_COLLAB_AGENT);
    expect(assignmentActiveProbeSql(`company_id=? AND role=? AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (? OR scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)`)).toBe(O_F1_WF_APPROVAL_SCOPE);
    expect(assignmentActiveProbeSql(`revoked_at IS NULL AND valid_until>?`)).toBe(O_F1_BETA_ASSIGNMENT);
    expect(`EXISTS(${assignmentUserScopeProbeSql(`iam_role_allows(role,company_id,'workflow.'||?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND
    (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?))))`)})`).toBe(O_F2_WF_GUARD26);
    expect(assignmentUserScopeProbeSql(`revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?))`)).toBe(O_F2_PMS_ASSIGNEE);
    expect(`EXISTS(${assignmentUserScopeProbeSql(`iam_role_allows(role,company_id,'company.owner') AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?`)})`).toBe(O_F2_BETA_INVITER);
    expect(`EXISTS(${assignmentUserScopeProbeSql(`revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?) AND NOT EXISTS(SELECT 1 ${peoplepayrollSources.people_directory_facts_hr_people_from} WHERE user_id=? AND company_id=? AND id!=? AND merged_into_id IS NULL`)})`).toBe(O_F2_PEOPLE_DIR_GUARD7);
    expect(assignmentUserScopeProbeSql(`iam_role_allows(role,company_id,'workflow.people_manager') AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?) AND (scope_kind='holding' OR scope_kind='branch' AND scope_node_id=? OR scope_kind='department' AND scope_node_id=?)`)).toBe(O_F2_OPS_ROUTE);
    expect(distinctAssigneeSql(`iam_role_allows(role,company_id,'company.owner') AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`)).toBe(O_F3_RECORDS_EMPLOYEE);
    expect(distinctAssigneeSql(`revoked_at IS NULL AND valid_from<=? AND valid_until>?`)).toBe(O_F3_COLLAB_TEXT);
    expect(distinctAssigneeSql(`user_id IS NOT NULL AND ((revoked_at IS NULL AND valid_from<=? AND valid_until>?) OR ?=1) AND (?::text IS NULL OR scope_kind='holding' OR COALESCE(branch_id,(SELECT branch_id ${organizationSources.member_directory_organization_nodes_from} WHERE id=scope_node_id))=?) ORDER BY user_id`)).toBe(O_F3_MEMBER_DIR);
    expect(distinctAssigneeSql(`iam_role_allows(role,company_id,'workflow.'||?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?
    AND (scope_kind='holding' OR (?='hr_admin' AND ((scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))) ORDER BY user_id`)).toBe(O_F3_OPS_ELIGIBLE);
  });

  test('keeps the three heads read-only (no writes or row locks)', () => {
    const all = [
      assignmentActiveProbeSql('company_id=?'),
      assignmentUserScopeProbeSql('revoked_at IS NULL'),
      distinctAssigneeSql('revoked_at IS NULL'),
    ].join(' ');
    expect(all).toContain('SELECT 1 FROM assignments');
    expect(all).toContain('SELECT DISTINCT user_id FROM assignments');
    expect(all).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('one definition per head; every call site uses it and no inline head remains', () => {
    const helper = read('adapters/read-tail-family-r29.ts');
    for (const name of ['assignmentActiveProbeSql', 'assignmentUserScopeProbeSql', 'distinctAssigneeSql']) {
      expect((helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).length, name).toBe(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(3);
    const counts: [string, string, number][] = [
      ['adapters/legal-ai-actor-facts.ts', 'assignmentActiveProbeSql', 4],
      ['adapters/workflow-command-authority.ts', 'assignmentActiveProbeSql', 2],
      ['adapters/records-request-ai-facts.ts', 'assignmentActiveProbeSql', 2],
      ['adapters/collaboration-agent-authority.ts', 'assignmentActiveProbeSql', 2],
      // Lane 12 wave 2 moved this read to the native owner; the guard uses the user-scope head.
      ['adapters/workflow-approvals-facts.ts', 'assignmentActiveProbeSql', 0],
      ['adapters/workflow-approvals-facts.ts', 'assignmentUserScopeProbeSql', 2],
      ['adapters/beta-invitation-sources.ts', 'assignmentActiveProbeSql', 2],
      ['adapters/beta-invitation-sources.ts', 'assignmentUserScopeProbeSql', 2],
      ['adapters/pms-command-facts.ts', 'assignmentUserScopeProbeSql', 2],
      ['adapters/people-directory-facts.ts', 'assignmentUserScopeProbeSql', 2],
      ['adapters/operations-staff-policy-facts.ts', 'assignmentUserScopeProbeSql', 2],
      ['adapters/operations-staff-policy-facts.ts', 'distinctAssigneeSql', 2],
      ['adapters/records-employee-requests-facts.ts', 'distinctAssigneeSql', 2],
      ['adapters/collaboration-text-sources.ts', 'distinctAssigneeSql', 2],
      ['adapters/member-directory.ts', 'distinctAssigneeSql', 2],
    ];
    for (const [rel, name, n] of counts) expect(calls(rel, name), `${rel} ${name}`).toBe(n);
    const heads = [
      'SELECT 1 FROM assignments WHERE id=? AND user_id=? AND ',
      'SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND ',
      'SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND ',
    ];
    for (const rel of [
      'adapters/legal-ai-actor-facts.ts', 'adapters/workflow-command-authority.ts',
      'adapters/records-request-ai-facts.ts', 'adapters/collaboration-agent-authority.ts',
      'adapters/workflow-approvals-facts.ts', 'adapters/beta-invitation-sources.ts',
      'adapters/pms-command-facts.ts', 'adapters/people-directory-facts.ts',
      'adapters/operations-staff-policy-facts.ts', 'adapters/records-employee-requests-facts.ts',
      'adapters/collaboration-text-sources.ts', 'adapters/member-directory.ts',
    ]) {
      const src = read(rel);
      for (const h of heads) expect(src.includes(h), `${rel} still inlines ${h}`).toBe(false);
    }
  });

  test('locks, session authority, native kinds and r1-r28 helpers stay untouched', () => {
    const helper = read('adapters/read-tail-family-r29.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(hash('adapters/product-sso-store.ts')).toBe('967b754535bb65e0196bf47fa0c0292c65b30276291e137c926d22579a3f3441');
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/native-session.ts')).toBe('1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1');
    expect(hash('adapters/read-tail-family-r28.ts')).toBe('6f5f736acb5cc6c796584ec86dfe52dcc40408ad0122be17bccfb8b2c7d1051d');
    expect(hash('adapters/read-tail-family-r27.ts')).toBe('f921a14d81bfc53158005b4fdc270deaa7580a276bb52d409a36a50995cbce76');
    expect(hash('adapters/pms-recipient-projection-family-r26.ts')).toBe('60917ed0d495413680ce96a5d97dcfff89ef31eaa6d8bef480c0e65161f1e7c4');
    expect(hash('adapters/active-role-probe-family-r25.ts')).toBe('aea7ee49d4c93ac3c904b5f8190119a277ad43921a21a70903cfdf5c0455d448');
    expect(hash('adapters/identity-user-projection-family-r18.ts')).toBe('55d639d10814e811b947cb23a30bf27055f5e150552f516365f36418568937a9');
    expect(hash('adapters/identity-user-projection-family-r21.ts')).toBe('cf0fdc9322d730d211a206f0a07e661c081743887d445d5b0ddc97ff41be0b04');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access read-tail r29 retirement check', () => {
  test('retirement:ts-statements is strictly below BEFORE=499 (expected 485)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(499);
    expect(total).toBeLessThanOrEqual(485);
  });
});
