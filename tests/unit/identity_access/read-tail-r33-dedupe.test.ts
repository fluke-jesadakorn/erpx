import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  activeWorkContextProbeSql,
  assignmentAuthorityProbeSql,
  assignmentCompanyUsersSql,
} from '../../../modules/identity-access/server/src/adapters/read-tail-family-r33';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Verbatim pre-r33 heads and tails captured from HEAD. SHA-256 values are
// byte-identity oracles only, never live query sources. An escaped `\${` in a
// tail below is the literal source text carried through the helper call.
const HEAD_AUTH = `SELECT 1 FROM assignments a WHERE a.id=? AND a.company_id=? AND a.user_id=? AND `;
const HEAD_USERS = `SELECT DISTINCT a.user_id FROM assignments a WHERE a.company_id=? AND `;
const HEAD_CTX = `SELECT 1 FROM active_work_contexts WHERE session_id=? AND company_id=? AND user_id=?`;

const HEADS = [
  { sql: HEAD_AUTH, sha256: 'c899fa4740ca749ead52d31f46455809913d7c884441ef6c48d3a52e4c3d260a' },
  { sql: HEAD_USERS, sha256: '75ab27cd01ac3aba9098fce8b9a0d1f2877dc1c4cc0c9773cd36f87058d9709f' },
  { sql: HEAD_CTX, sha256: 'fbf6f6a58a1e491186126d204dd56fe58fe88244ad618c1d2558a7e5baaaf705' },
] as const;

const TAIL_PAY = `a.role=? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?) AND iam_role_allows(a.role,a.company_id,?) AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?)`;
const TAIL_REC = `a.role=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,?)`;
const TAIL_APPROVALS = `a.revoked_at IS NULL AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (iam_role_allows(a.role,a.company_id,'workflow.people_manager') AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=? OR a.scope_kind='department' AND a.scope_node_id=?) OR EXISTS(SELECT 1 \${organizationFinalSources.workflow_approvals_facts_authority_seats_s_from} \${peoplepayrollSources.workflow_approvals_facts_hr_people_p_join} ON p.id=s.person_id AND p.company_id=s.company_id \${engagementJoin}WHERE s.company_id=a.company_id AND s.kind IN ('ceo','hr') AND iam_same_person(p.user_id,a.user_id,a.company_id) AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint))`;
const TAIL_MSG = `a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? \${teamId?messagingTeamMemberPredicate:''}`;
const TAIL_SUBJ = `iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ORDER BY a.user_id`;
const TAIL_LEGAL = `\n    AND assignment_id=? AND revision=? AND assignment_version=? AND expires_at>?\n    AND NOT EXISTS(\${revokedSessionProbeSql('?','?','?')})`;
const TAIL_WF = ` AND assignment_id=? AND revision=? AND assignment_version=? AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint`;

const SITES = [
  {
    file: 'adapters/payroll-payslip-grant.ts', helper: 'assignmentAuthorityProbeSql',
    head: HEAD_AUTH, tail: TAIL_PAY, wrap: false,
    render: (t: string) => assignmentAuthorityProbeSql(t),
    sha256: '167d30d9f5dce4e989b547b80e66ecde097551a7d7e568ff3cd420331200c0fe',
    binds: '.bind(assignmentId,companyId,userId,role,now,now2,permission,branchId,departmentId)',
  },
  {
    file: 'adapters/payroll-recovery-grant.ts', helper: 'assignmentAuthorityProbeSql',
    head: HEAD_AUTH, tail: TAIL_REC, wrap: true,
    render: (t: string) => `EXISTS(${assignmentAuthorityProbeSql(t)})`,
    sha256: 'e13522a9a29714f3426ea3972a2f1644ed78b328ac257675fbc5625c5d3e1fe4',
    binds: '.bind(assignmentId,companyId,userId,role,permission)',
  },
  {
    file: 'adapters/workflow-approvals-facts.ts', helper: 'assignmentAuthorityProbeSql',
    head: HEAD_AUTH, tail: TAIL_APPROVALS, wrap: true,
    render: (t: string) => `EXISTS(${assignmentAuthorityProbeSql(t)})`,
    sha256: '91e5ac2d4c307d2201c7c9ef670378331d419aae9508bc4f1a3e18764cf7917e',
    binds: '[assignmentId,companyId,userId,branchId,departmentId]',
  },
  {
    file: 'adapters/collaboration-messaging-authorization.ts', helper: 'assignmentCompanyUsersSql',
    head: HEAD_USERS, tail: TAIL_MSG, wrap: false,
    render: (t: string) => assignmentCompanyUsersSql(t),
    sha256: 'de2d8ac10ea2a8c98102d5f5d43b34bace6b00a484267924f52a9c933631f123',
    binds: '.bind(companyId,now,now2,...values)',
  },
  {
    file: 'adapters/project-worker-startup-subject.ts', helper: 'assignmentCompanyUsersSql',
    head: HEAD_USERS, tail: TAIL_SUBJ, wrap: false,
    render: (t: string) => assignmentCompanyUsersSql(t),
    sha256: '584edeb24c9bfd9808d6948221277c418dc7dde92879329cabbfd6563717b152',
    binds: '.bind(companyId)',
  },
  {
    file: 'adapters/legal-ai-actor-facts.ts', helper: 'activeWorkContextProbeSql',
    head: HEAD_CTX, tail: TAIL_LEGAL, wrap: false,
    render: (t: string) => activeWorkContextProbeSql(t),
    sha256: '2ea1dbf680b41c44e0a9071cc0814a218c7849baa50c70666b5eba305204cf8a',
    binds: '.bind(facts.sessionId,facts.companyId,facts.actorId,facts.assignmentId,facts.revision,facts.assignmentVersion,facts.now,facts.sessionId2,facts.companyId2,facts.actorId2)',
  },
  {
    file: 'adapters/workflow-command-authority.ts', helper: 'activeWorkContextProbeSql',
    head: HEAD_CTX, tail: TAIL_WF, wrap: true,
    render: (t: string) => `EXISTS(${activeWorkContextProbeSql(t)})`,
    sha256: 'a16491f19581c09f922861a5fca304fc9a5445ad9bb4c87ba4e2e606eae4adb9',
    binds: '[actor.authority.sessionId,actor.companyId,actor.userId,actor.assignmentId,actor.authority.revision,actor.authority.assignmentVersion]',
  },
] as const;

describe('Identity access read-tail r33 byte oracles', () => {
  test('three pinned heads render byte-identically at the empty tail', () => {
    for (const h of HEADS) expect(sha256(h.sql)).toBe(h.sha256);
    expect(assignmentAuthorityProbeSql('')).toBe(HEAD_AUTH);
    expect(assignmentCompanyUsersSql('')).toBe(HEAD_USERS);
    expect(activeWorkContextProbeSql('')).toBe(HEAD_CTX);
  });

  test('all seven rendered statements recompose byte-for-byte from their pre-dedupe literals', () => {
    for (const s of SITES) {
      const body = `${s.head}${s.tail}`;
      const full = s.wrap ? `EXISTS(${body})` : body;
      expect(sha256(full), s.file).toBe(s.sha256);
      expect(s.render(s.tail), s.file).toBe(full);
    }
  });

  test('newline tail and revokedSessionProbeSql composition are preserved', () => {
    expect(TAIL_LEGAL).toContain('\n    AND assignment_id=?');
    expect(TAIL_LEGAL).toContain(`NOT EXISTS(\${revokedSessionProbeSql('?','?','?')})`);
    expect(sha256(TAIL_LEGAL)).toBe('a0a745008f0f05dd6a1cbcc0d22312aa502d20e7374b9f1508c21640e7b6787c');
    const legal = read('adapters/legal-ai-actor-facts.ts');
    expect(legal).toContain(`NOT EXISTS(\${revokedSessionProbeSql('?','?','?')})`);
    expect(calls('adapters/legal-ai-actor-facts.ts', 'revokedSessionProbeSql')).toBe(2);
    expect(legal).toContain('.bind(facts.sessionId,facts.companyId,facts.actorId,');
  });
});

describe('Identity access read-tail r33 wiring', () => {
  test('one head per helper; all seven call sites use them and keep tails and binds', () => {
    const helper = read('adapters/read-tail-family-r33.ts');
    for (const name of ['assignmentAuthorityProbeSql', 'assignmentCompanyUsersSql', 'activeWorkContextProbeSql']) {
      expect((helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).length, name).toBe(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(3);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    for (const s of SITES) {
      const src = read(s.file);
      expect(calls(s.file, s.helper), `${s.file} ${s.helper}`).toBe(2);
      expect(src.includes(`${s.helper}(`), `${s.file} routes through ${s.helper}`).toBe(true);
      expect(src.includes(s.tail), `${s.file} keeps its verbatim tail`).toBe(true);
      expect(src.includes(s.binds), `${s.file} keeps its binds`).toBe(true);
      expect(src.includes(s.head), `${s.file} still inlines ${s.head}`).toBe(false);
    }
  });

  test('locks, the newline-shape cancellation grant and earlier-round helpers stay untouched', () => {
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/read-tail-family-r29.ts')).toBe('2c392291ac6bfa7558a022f750fba0742acfe7014702875d8836321063650775');
    expect(hash('adapters/read-tail-family-r30.ts')).toBe('2cde8f94073ca753169c63c8024ea217282a953d2420a1ae6f60c056a11675a0');
    expect(hash('adapters/read-tail-family-r31.ts')).toBe('83c107b5cd94bcc29a51b14777dfa01675ada8b1d059c43bbfe8bb009e8cb9c4');
    expect(hash('adapters/read-tail-family-r32.ts')).toBe('0b07e2d8c5d151446acd0ff741e14ad242b18c708b194d7e13d7a63aac49bf3d');
    expect(read('adapters/payroll-cancellation-grant.ts')).toContain("EXISTS(SELECT 1 FROM assignments a\n  WHERE a.id=?");
    expect(read('adapters/payroll-cancellation-grant.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT session_id FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });

  test('post-dedupe files match their pinned byte oracles', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r33.ts', '3b6789a67a614277f39bedad7411a89d048870f41d5438ab88779e94a9b9e52d'],
      ['adapters/payroll-payslip-grant.ts', '3135c8ee936025148f0bcdc50b7a0ffbd215f26ee9adde00ef8692ecfb4bcef3'],
      ['adapters/payroll-recovery-grant.ts', 'f8c30abffc5fa9ba1d93f50fd2c32794d9645cf39df5387cae3ce874178ae0c0'],
      ['adapters/workflow-approvals-facts.ts', 'ede40e8b130d2ea56dfc304622fe30e8cba457c5aeeeb90d2c39cf66497a1044'],
      ['adapters/collaboration-messaging-authorization.ts', 'a424bde90984f30c8d7287b17d05180d0d33b7694d652913b9dd3f72485d2cea'],
      ['adapters/project-worker-startup-subject.ts', 'c83fd8ea6b1a360d859695448d95bba90ffefcecc6f221615d9b45361c559c75'],
      ['adapters/legal-ai-actor-facts.ts', 'fb14fcf39793fe15e7ebda1fa64bc0f51dd8aa416d2987ac461b7ad32e9bfa07'],
      ['adapters/workflow-command-authority.ts', 'd0939ee195594843eb159709f7214996667b7174d418235750e4ca09c93268ca'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
  });
});

describe('Identity access read-tail r33 retirement check', () => {
  test('retirement:ts-statements is 368 after access-diagnosis plan cut', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(369);
    expect(total).toBe(368);
  });
});
