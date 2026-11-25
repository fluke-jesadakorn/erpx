import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  userIdNameEmailInSql, iamReviewProbeSql, iamGroupProbeSql, iamMfaProbeSql,
  peopleDistinctNameSql, assignmentVersionSql, assignmentRoleSql,
} from '../../../packages/server-identity_access/src/adapters/read-tail-family-r28';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const helper = read('adapters/read-tail-family-r28.ts');
const esc = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Pre-dedupe literals captured from HEAD. Rendered helper output must match each exactly.
const PW = "SELECT id,name,email FROM \"user\" WHERE id IN (?,?) AND (?='' OR position(lower(?) in lower(name||' '||email))>0) ORDER BY lower(name),id LIMIT 50";
const PA = "SELECT id,name,email FROM \"user\" WHERE id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY name,id";
const MA = "SELECT id,name,email FROM \"user\" WHERE id IN (${ids.map(()=>'?').join(',')}) ORDER BY name,id";
const API_REVIEW = "SELECT 1 FROM iam_reviews WHERE id=? AND company_id=? AND kind='policy.grant'";
const PLF_REVIEW = "SELECT 1 FROM iam_reviews WHERE company_id=$1 AND payload->>'caseId'=$2 AND status IN ('pending','approved')";
const DM_GROUP = "SELECT 1 FROM iam_groups g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=$1 AND c.id=$2 AND g.external_id=$3";
const PLF_GROUP = "SELECT 1 FROM iam_groups WHERE connection_id=$1 AND external_id=$2";
const API_MFA = "SELECT 1 FROM iam_mfa WHERE user_id=? AND enabled";
const MIG_MFA = "SELECT 1 FROM iam_mfa m JOIN account a ON a.\"userId\"=m.user_id WHERE m.user_id=? AND m.enabled AND a.\"providerId\"='credential' AND a.password IS NOT NULL";
const RET_PEOPLE = "SELECT DISTINCT a.user_id id,coalesce(p.legal_name,a.user_id) name,k.kind FROM assignments a LEFT ${peoplepayrollSources.retirement_clearance_facts_hr_people_p_join} ON p.user_id=a.user_id AND p.company_id=a.company_id AND p.merged_into_id IS NULL CROSS JOIN (VALUES ('hr','people.manage'),('handover',NULL),('equipment',NULL),('directory','iam.directory.manage'),('payroll','payroll.prepare'),('finance','finance.write')) k(kind,permission) WHERE a.company_id=? AND iam_assignment_active(a.id,?) AND (k.permission IS NULL OR iam_role_allows(a.role,a.company_id,k.permission)) AND (k.kind NOT IN ('payroll','finance','directory') OR a.scope_kind='holding') ORDER BY name";
const LEGAL_PEOPLE = "SELECT DISTINCT a.user_id id,coalesce(p.legal_name,a.user_id) name,a.role\n    FROM assignments a LEFT ${peoplepayrollSources.legal_people_source_hr_people_p_join} ON p.user_id=a.user_id AND p.company_id=a.company_id AND p.merged_into_id IS NULL\n    WHERE a.company_id=? AND legal_person_eligible(a.company_id,a.user_id,?,?,?)\n    AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?)\n    AND (?::text IS NULL OR iam_role_allows(a.role,a.company_id,?))\n    AND (?::text IS NULL OR a.role=?)\n    AND (?='' OR strpos(lower(coalesce(p.legal_name,a.user_id)),lower(?))>0)\n    AND (?::text IS NULL OR ?!='reviewer' OR EXISTS(SELECT 1 ${legalSources.legal_people_source_legal_members_m_from} WHERE m.item_id=? AND m.user_id=a.user_id))\n    ORDER BY name,id,a.role LIMIT 100";
const FIN_VERSION = "SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?";
const JOBS_VERSION = "SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND iam_assignment_active(id,?)";
const ACC_VERSION = "SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=?";
const POS_ROLE = "SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?";
const RAI_ROLE = "SELECT role FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)";

const LEGAL_HEAD = 'SELECT DISTINCT a.user_id id,coalesce(p.legal_name,a.user_id) name,';

describe('Identity access read-tail r28 seven parameterized families', () => {
  test('each rendered head is byte-identical to its pre-dedupe literal', () => {
    const pwTail = "?,?) AND (?='' OR position(lower(?) in lower(name||' '||email))>0) ORDER BY lower(name),id LIMIT 50";
    expect(userIdNameEmailInSql(pwTail)).toBe(PW);
    expect(userIdNameEmailInSql('SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY name,id')).toBe(PA);
    expect(userIdNameEmailInSql("${ids.map(()=>'?').join(',')}) ORDER BY name,id")).toBe(MA);
    expect(iamReviewProbeSql("id=? AND company_id=? AND kind='policy.grant'")).toBe(API_REVIEW);
    expect(iamReviewProbeSql("company_id=$1 AND payload->>'caseId'=$2 AND status IN ('pending','approved')")).toBe(PLF_REVIEW);
    expect(iamGroupProbeSql('g JOIN iam_connections c ON c.id=g.connection_id WHERE c.company_id=$1 AND c.id=$2 AND g.external_id=$3')).toBe(DM_GROUP);
    expect(iamGroupProbeSql('WHERE connection_id=$1 AND external_id=$2')).toBe(PLF_GROUP);
    expect(iamMfaProbeSql('WHERE user_id=? AND enabled')).toBe(API_MFA);
    expect(iamMfaProbeSql('m JOIN account a ON a."userId"=m.user_id WHERE m.user_id=? AND m.enabled AND a."providerId"=\'credential\' AND a.password IS NOT NULL')).toBe(MIG_MFA);
    expect(peopleDistinctNameSql(RET_PEOPLE.slice(LEGAL_HEAD.length))).toBe(RET_PEOPLE);
    expect(peopleDistinctNameSql(LEGAL_PEOPLE.slice(LEGAL_HEAD.length))).toBe(LEGAL_PEOPLE);
    expect(assignmentVersionSql("company_id=? AND role=? AND scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?")).toBe(FIN_VERSION);
    expect(assignmentVersionSql('company_id=? AND iam_assignment_active(id,?)')).toBe(JOBS_VERSION);
    expect(assignmentVersionSql('company_id=?')).toBe(ACC_VERSION);
    expect(assignmentRoleSql("scope_kind='holding' AND revoked_at IS NULL AND valid_from<=? AND valid_until>?")).toBe(POS_ROLE);
    expect(assignmentRoleSql("role=? AND (?::bigint IS NULL OR version=?) AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=(floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_until>(floor(extract(epoch FROM clock_timestamp())*1000)::bigint)")).toBe(RAI_ROLE);
  });

  test('one owner helper defines exactly seven heads; every call site uses it and no old literal remains', () => {
    expect((helper.match(/SELECT /g) ?? []).length).toBe(7);
    for (const name of [
      'userIdNameEmailInSql', 'iamReviewProbeSql', 'iamGroupProbeSql', 'iamMfaProbeSql',
      'peopleDistinctNameSql', 'assignmentVersionSql', 'assignmentRoleSql',
    ]) expect((helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).length, name).toBe(1);

    expect(calls('adapters/project-worker-startup-subject.ts', 'userIdNameEmailInSql')).toBe(2);
    expect(calls('adapters/people-access.ts', 'userIdNameEmailInSql')).toBe(2);
    expect(calls('adapters/collaboration-messaging-accounts.ts', 'userIdNameEmailInSql')).toBe(2);
    expect(calls('adapters/iam-api.ts', 'iamReviewProbeSql')).toBe(2);
    expect(calls('adapters/people-lifecycle-facts.ts', 'iamReviewProbeSql')).toBe(2);
    expect(calls('adapters/directory-mapping.ts', 'iamGroupProbeSql')).toBe(2);
    expect(calls('adapters/people-lifecycle-facts.ts', 'iamGroupProbeSql')).toBe(2);
    expect(calls('adapters/iam-api.ts', 'iamMfaProbeSql')).toBe(2);
    expect(calls('adapters/iam-migrations.ts', 'iamMfaProbeSql')).toBe(2);
    expect(calls('adapters/retirement-clearance-facts.ts', 'peopleDistinctNameSql')).toBe(2);
    expect(calls('adapters/legal-people-source.ts', 'peopleDistinctNameSql')).toBe(2);
    expect(calls('adapters/finance-delegation.ts', 'assignmentVersionSql')).toBe(2);
    expect(calls('adapters/iam-jobs.ts', 'assignmentVersionSql')).toBe(2);
    expect(calls('adapters/account-access.ts', 'assignmentVersionSql')).toBe(2);
    expect(calls('adapters/pos-stripe-facts.ts', 'assignmentRoleSql')).toBe(2);
    expect(calls('adapters/records-request-ai-facts.ts', 'assignmentRoleSql')).toBe(2);

    const inline = [PW, PA, MA, API_REVIEW, PLF_REVIEW, DM_GROUP, PLF_GROUP, API_MFA, MIG_MFA, RET_PEOPLE, LEGAL_PEOPLE, FIN_VERSION, JOBS_VERSION, ACC_VERSION, POS_ROLE, RAI_ROLE];
    for (const rel of [
      'adapters/project-worker-startup-subject.ts', 'adapters/people-access.ts', 'adapters/collaboration-messaging-accounts.ts',
      'adapters/iam-api.ts', 'adapters/people-lifecycle-facts.ts', 'adapters/directory-mapping.ts', 'adapters/iam-migrations.ts',
      'adapters/retirement-clearance-facts.ts', 'adapters/legal-people-source.ts', 'adapters/finance-delegation.ts',
      'adapters/iam-jobs.ts', 'adapters/account-access.ts', 'adapters/pos-stripe-facts.ts', 'adapters/records-request-ai-facts.ts',
    ]) {
      const src = read(rel);
      for (const literal of inline) expect(new RegExp(`${esc(literal)}['"\`]`).test(src), `${rel} still inlines ${literal.slice(0, 60)}`).toBe(false);
    }
  });

  test('locks, session authority, native kinds and earlier helpers are untouched', () => {
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    // Lock statements keep their byte identity.
    expect(hash('adapters/product-sso-store.ts')).toBe('967b754535bb65e0196bf47fa0c0292c65b30276291e137c926d22579a3f3441');
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    expect(hash('adapters/workflow-command-authority.ts')).toBe('d0939ee195594843eb159709f7214996667b7174d418235750e4ca09c93268ca');
    expect(read('adapters/iam-migrations.ts')).toContain('SELECT * FROM iam_connections WHERE id=$1 AND company_id=$2 AND status=\'active\' AND last_sync_at>$3 FOR SHARE');
    // Session authority families keep their byte identity.
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/native-session.ts')).toBe('1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1');
    // Native (Rust) kinds and earlier-round helpers are not edited.
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
    expect(hash('adapters/pms-recipient-projection-family-r26.ts')).toBe('60917ed0d495413680ce96a5d97dcfff89ef31eaa6d8bef480c0e65161f1e7c4');
    expect(hash('adapters/active-role-probe-family-r25.ts')).toBe('aea7ee49d4c93ac3c904b5f8190119a277ad43921a21a70903cfdf5c0455d448');
    expect(hash('adapters/read-tail-family-r27.ts')).toBe('f921a14d81bfc53158005b4fdc270deaa7580a276bb52d409a36a50995cbce76');
    expect(hash('adapters/identity-user-projection-family-r18.ts')).toBe('55d639d10814e811b947cb23a30bf27055f5e150552f516365f36418568937a9');
    expect(hash('adapters/identity-user-projection-family-r21.ts')).toBe('cf0fdc9322d730d211a206f0a07e661c081743887d445d5b0ddc97ff41be0b04');
  });
});

describe('Identity access read-tail r28 retirement check', () => {
  test('retirement:ts-statements is strictly lower than BEFORE=508 (r29 lowered 499 to 485)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(508);
    expect(total).toBeLessThanOrEqual(485);
  });
});
