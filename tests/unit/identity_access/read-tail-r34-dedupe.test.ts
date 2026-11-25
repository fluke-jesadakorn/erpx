import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  iamMemberVersionProbeSql,
  iamServiceNodeSubquerySql,
  workflowRoleNodeProbeSql,
} from '../../../packages/server-identity_access/src/adapters/read-tail-family-r34';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Verbatim pre-r34 heads captured from HEAD. SHA-256 values are byte-identity
// oracles only, never live query sources. An escaped `${` in a head below is
// the literal source text carried through the helper call.
const WF_FROM = '${organizationSources.workflow_role_facts_organization_nodes_from}';
const SVC_FROM = '${organizationSources.iam_service_organization_nodes_from}';

const HEADS = [
  { name: 'wfBranch', sql: `SELECT 1 ${WF_FROM} WHERE id=? AND company_id=staff.company_id AND kind='branch' AND status='active'`, sha256: '447bcc0b1b85f00f63bf1bdcd020c38cfaf93d53e1bee3f5f85dbc70377e7c9b' },
  { name: 'wfDept', sql: `SELECT 1 ${WF_FROM} WHERE id=? AND company_id=staff.company_id AND kind='department' AND status='active'`, sha256: 'c379bc70ab8046bd9e032aa6b3aee23561f59ab2018898366fdb0860a522f3ad' },
  { name: 'svcBranchCompany', sql: `SELECT id ${SVC_FROM} WHERE branch_id=? AND company_id=?`, sha256: 'fec1a8d57e4cdb6a2fbeaeefe3a578d642bdd61802db0d60bdc1cfd2d85bb789' },
  { name: 'svcCompanyBranch', sql: `SELECT id ${SVC_FROM} WHERE company_id=? AND branch_id=?`, sha256: '3e67ba9fb70d2269ee85976288b209b518f4fc0800b486ddce0bf749baba499a' },
  { name: 'mvCast', sql: 'SELECT version FROM iam_members WHERE company_id=?::text AND user_id=?::text', sha256: '5190c1793ffaed8fd14c979781ee7d3cef883fcd05f4e3d10cad5f5acda0c558' },
  { name: 'mvSocial', sql: 'SELECT version FROM iam_members m WHERE m.company_id=${company} AND m.user_id=${user}', sha256: 'ae5f479f9503f4c8aeb9047640c94b645c92fde98ffe3c0133159022c558f24b' },
  { name: 'mvGrant', sql: 'SELECT version FROM iam_members WHERE company_id=grant_row.company_id AND user_id=grant_row.user_id', sha256: '0e0a94e478551bc5f432543beb5fdc2d449d6821f46a05e88619139e276d4c50' },
  { name: 'mvPlain', sql: 'SELECT version FROM iam_members WHERE company_id=? AND user_id=?', sha256: 'f2ad98ba42f861a1fbde3cd0aa1a03414013eaed46561e5e836b5e9d43d5e1b3' },
  { name: 'mvReport', sql: 'SELECT m.version FROM iam_members m WHERE m.company_id=g.company_id AND m.user_id=g.user_id', sha256: 'da42b513f9fa635d8bf16f438685922cfef8720868bbb901902be2a4e56586b7' },
] as const;

// Each entry: file, helper name, exact rendered call expression (source text),
// the inline head it replaced, and the unchanged bind expression.
const SITES = [
  {
    file: 'adapters/workflow-role-facts.ts', helper: 'workflowRoleNodeProbeSql',
    call: "workflowRoleNodeProbeSql(organizationSources.workflow_role_facts_organization_nodes_from,'branch')",
    head: HEADS[0].sql, wraps: 'EXISTS(', uses: 2,
    bind: '.bind(scope.company_id,now,now,scope.company_id,now,now,scope.branch_id,scope.department_id,scope.branch_id,scope.branch_id,scope.department_id,scope.department_id)',
  },
  {
    file: 'adapters/workflow-role-facts.ts', helper: 'workflowRoleNodeProbeSql',
    call: "workflowRoleNodeProbeSql(organizationSources.workflow_role_facts_organization_nodes_from,'department')",
    head: HEADS[1].sql, wraps: 'EXISTS(', uses: 2,
    bind: 'scope.department_id,',
  },
  {
    file: 'adapters/iam-service-reads.ts', helper: 'iamServiceNodeSubquerySql',
    call: "iamServiceNodeSubquerySql(organizationSources.iam_service_organization_nodes_from,'branch-first')",
    head: HEADS[2].sql, wraps: 'IN (', uses: 2,
    bind: '.bind(actor.companyId, ...(actor.platform ? [] : [actor.companyId]), self ? actor.userId : null, self ? actor.userId : null,',
  },
  {
    file: 'adapters/iam-service-reads.ts', helper: 'iamServiceNodeSubquerySql',
    call: "iamServiceNodeSubquerySql(organizationSources.iam_service_organization_nodes_from,'company-first')",
    head: HEADS[3].sql, wraps: 'IN (', uses: 1,
    bind: '.bind(actor.companyId, hasPermission(actor, iamPermission(actor, "iam.review")) &&',
  },
  {
    file: 'adapters/iam-store.ts', helper: 'iamMemberVersionProbeSql',
    call: "iamMemberVersionProbeSql('cast')",
    head: HEADS[4].sql, wraps: 'COALESCE((', uses: 1,
    bind: 'iam?.identityVersion ?? null,actor.companyId,actor.userId,iam?.identityVersion ?? null,actor.role',
  },
  {
    file: 'adapters/queued-social-source-authority.ts', helper: 'iamMemberVersionProbeSql',
    call: "iamMemberVersionProbeSql('interp',company,user)",
    head: HEADS[5].sql, wraps: 'COALESCE((', uses: 1,
    bind: ")=${number(`${iam}->>'identityVersion'`)}",
  },
  {
    file: 'adapters/executive-authority-source.ts', helper: 'iamMemberVersionProbeSql',
    call: "iamMemberVersionProbeSql('grant')",
    head: HEADS[6].sql, wraps: 'COALESCE((', uses: 1,
    bind: 'iam?.identityVersion??null,iam?.identityVersion??null,',
  },
  {
    file: 'adapters/record-source-authority.ts', helper: 'iamMemberVersionProbeSql',
    call: "iamMemberVersionProbeSql('plain')",
    head: HEADS[7].sql, wraps: 'COALESCE((', uses: 1,
    bind: 'iam?.identityVersion??null,actor.companyId,actor.userId,iam?.identityVersion??null,iam?.emergencyUntil??null',
  },
  {
    file: 'adapters/executive-report-sources.ts', helper: 'iamMemberVersionProbeSql',
    call: "iamMemberVersionProbeSql('report')",
    head: HEADS[8].sql, wraps: 'COALESCE((', uses: 1,
    bind: 'guard33(key: string, value: "transfer-target"',
  },
] as const;

describe('Identity access read-tail r34 byte oracles', () => {
  test('all nine pinned heads carry the recorded SHA-256 and render byte-identically', () => {
    for (const h of HEADS) expect(sha256(h.sql), h.name).toBe(h.sha256);
    const rendered = [
      workflowRoleNodeProbeSql(WF_FROM, 'branch'),
      workflowRoleNodeProbeSql(WF_FROM, 'department'),
      iamServiceNodeSubquerySql(SVC_FROM, 'branch-first'),
      iamServiceNodeSubquerySql(SVC_FROM, 'company-first'),
      iamMemberVersionProbeSql('cast'),
      iamMemberVersionProbeSql('interp', '${company}', '${user}'),
      iamMemberVersionProbeSql('grant'),
      iamMemberVersionProbeSql('plain'),
      iamMemberVersionProbeSql('report'),
    ];
    rendered.forEach((sql, i) => {
      expect(sql, HEADS[i].name).toBe(HEADS[i].sql);
      expect(sha256(sql), HEADS[i].name).toBe(HEADS[i].sha256);
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    });
  });

  test('closed unions keep bind placeholders and WHERE order intact', () => {
    expect(iamServiceNodeSubquerySql(SVC_FROM, 'branch-first')).toContain('WHERE branch_id=? AND company_id=?');
    expect(iamServiceNodeSubquerySql(SVC_FROM, 'company-first')).toContain('WHERE company_id=? AND branch_id=?');
    expect(iamMemberVersionProbeSql('cast')).toContain('company_id=?::text AND user_id=?::text');
    expect(iamMemberVersionProbeSql('plain')).toContain('company_id=? AND user_id=?');
    expect(iamMemberVersionProbeSql('grant')).toContain('company_id=grant_row.company_id AND user_id=grant_row.user_id');
    expect(iamMemberVersionProbeSql('report')).toContain('m.company_id=g.company_id AND m.user_id=g.user_id');
    expect(iamMemberVersionProbeSql('interp', 'A', 'B')).toBe('SELECT version FROM iam_members m WHERE m.company_id=A AND m.user_id=B');
  });
});

describe('Identity access read-tail r34 wiring', () => {
  test('one helper head per family; every call site routes through it and keeps its tail and binds', () => {
    const helper = read('adapters/read-tail-family-r34.ts');
    for (const name of ['workflowRoleNodeProbeSql', 'iamServiceNodeSubquerySql', 'iamMemberVersionProbeSql']) {
      expect((helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).length, name).toBe(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(3);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const bySite = new Map<string, number>();
    for (const s of SITES) {
      const src = read(s.file);
      expect(src.includes(`${s.call}`), `${s.file} routes through ${s.call}`).toBe(true);
      expect(src.includes(s.bind), `${s.file} keeps its binds`).toBe(true);
      expect(src.includes(s.head), `${s.file} no longer inlines ${s.head}`).toBe(false);
      const key = `${s.file}|${s.helper}`;
      bySite.set(key, (bySite.get(key) ?? 0) + s.uses);
    }
    for (const [key, uses] of bySite) {
      const [file, helper] = key.split('|');
      expect(calls(file, helper), `${file} ${helper}`).toBe(uses + 1); // import + uses
    }
  });

  test('locks, session authority, native kinds and r1-r33 helpers stay untouched', () => {
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/read-tail-family-r27.ts')).toBe('f921a14d81bfc53158005b4fdc270deaa7580a276bb52d409a36a50995cbce76');
    expect(hash('adapters/read-tail-family-r28.ts')).toBe('6f5f736acb5cc6c796584ec86dfe52dcc40408ad0122be17bccfb8b2c7d1051d');
    expect(hash('adapters/read-tail-family-r29.ts')).toBe('2c392291ac6bfa7558a022f750fba0742acfe7014702875d8836321063650775');
    expect(hash('adapters/read-tail-family-r30.ts')).toBe('2cde8f94073ca753169c63c8024ea217282a953d2420a1ae6f60c056a11675a0');
    expect(hash('adapters/read-tail-family-r31.ts')).toBe('83c107b5cd94bcc29a51b14777dfa01675ada8b1d059c43bbfe8bb009e8cb9c4');
    expect(hash('adapters/read-tail-family-r32.ts')).toBe('0b07e2d8c5d151446acd0ff741e14ad242b18c708b194d7e13d7a63aac49bf3d');
    expect(hash('adapters/read-tail-family-r33.ts')).toBe('3b6789a67a614277f39bedad7411a89d048870f41d5438ab88779e94a9b9e52d');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT session_id FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/payroll-cancellation-grant.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });

  test('post-dedupe files match their pinned byte oracles', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r34.ts', '95e25ead2dac0a1832535f73794691036025e14748f1c17ac342c9620b3750be'],
      ['adapters/workflow-role-facts.ts', 'be93ded1066a437cdc5f3f6e5158f318599e9ad86bfa018bd51424c4fffaddc9'],
      ['adapters/iam-service-reads.ts', '8e9e15a6e5cf9544236d8b54516d1f07ba119b479fc27a1ec2a98d85dc59532b'],
      ['adapters/iam-store.ts', '827ec154f6723367e4974fa0e58fbf86843b779fd68166a6c8897b8a48c3c1fd'],
      ['adapters/queued-social-source-authority.ts', 'a5379950fb6735056405839f43758dd14235fc8da7402097cb120e780a77663e'],
      ['adapters/executive-authority-source.ts', 'd6526a6b37bdcaa1b17eb55e319bf936b5431d46e8ebaa8b142adcc57baa6170'],
      ['adapters/record-source-authority.ts', 'c49bb0b548f25381a932bd3532dfe19b63724b67b14f33035f597ac11da84251'],
      ['adapters/executive-report-sources.ts', 'b815a8d4913c62d311af1f876f946e322e529070fa579b7a4450725dd9a63bd5'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
  });
});

describe('Identity access read-tail r34 retirement check', () => {
  test('retirement:ts-statements is 438 after r41=439 (r35 lowered 459 to 454)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(468);
    expect(total).toBe(438);
  });
});
