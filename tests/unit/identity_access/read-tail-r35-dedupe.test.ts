import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  hrSeatExistsSql,
  iamRoleAllowsAliasSql,
  scopeBranchExistsSql,
} from '../../../packages/server-identity_access/src/adapters/read-tail-family-r35';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Verbatim pre-r35 heads captured from HEAD+ r34. SHA-256 values are
// byte-identity oracles only, never live query sources. An escaped `${` is the
// literal interpolation text carried through the helper call.
const HEADS = [
  {
    name: 'scopeA',
    sql: `SELECT 1 \${organizationSources.authorization_organization_nodes_b_from}\n                    WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active'`,
    sha256: '0cb401a837d3403ee8b2dfb74aa7c48f4aa86f074219895e17e81dc88ba36dec',
  },
  {
    name: 'scopeGrant',
    sql: `SELECT 1 \${organizationSources.executive_authority_source_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.company_id=grant_row.company_id AND b.status='active'`,
    sha256: '3e6d286fff6f95b3396237b41e08a2552d6b69b3ea0fba7049cfd6c2a942e9ff',
  },
  {
    name: 'scopeG',
    sql: `SELECT 1 \${organizationSources.executive_report_sources_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.company_id=g.company_id AND b.status='active'`,
    sha256: '151fb2e75994452c8c000075a0c4b91b4b148434e58392bbe7c3dc3daf2a4264',
  },
  {
    name: 'scopeA2',
    sql: `SELECT 1 \${organizationSources.workflow_command_authority_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active'`,
    sha256: 'd844478735fdb8332446cf33d780fe38630df72fa50e2b92725f33ddceb55dcc',
  },
  {
    name: 'hrPrefix',
    sql: `SELECT 1 \${organization.iam_store_authority_seats_s_from} \${people.iam_store_hr_people_p_join} ON p.id=s.person_id AND p.company_id=s.company_id AND p.status='active' AND p.merged_into_id IS NULL \${people.iam_store_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active'`,
    sha256: '9a18040d931258467c1011189a9bc0d2d22eb3735179d505e05ab46ee5252c29',
  },
  { name: 'roleAllowed', sql: 'SELECT iam_role_allows($1,$2,$3) allowed', sha256: '3f90049c4c4dc7ada1a935e208e9640ea358f305da8a877c603f24d308a06522' },
  { name: 'roleOk', sql: 'SELECT iam_role_allows($1,$2,$3) ok', sha256: '4a8322af0c1c1ce4e86104e043a0ac019be1c06567adb19efa1d5a539431575e' },
] as const;

const SEATS = '${organization.iam_store_authority_seats_s_from}';
const PJOIN = '${people.iam_store_hr_people_p_join}';
const EJOIN = '${people.iam_store_hr_engagements_e_join}';

const SITES = [
  {
    file: 'adapters/account-authority-snapshot.ts', helper: 'scopeBranchExistsSql',
    call: "scopeBranchExistsSql(organizationSources.authorization_organization_nodes_b_from,'a-multiline')",
    head: HEADS[0].sql, uses: 1, bind: '.bind(companyId,userId,now,sessionId??null,Date.now()-300000)',
  },
  {
    file: 'adapters/executive-authority-source.ts', helper: 'scopeBranchExistsSql',
    call: "scopeBranchExistsSql(organizationSources.executive_authority_source_organization_nodes_b_from,'grant')",
    head: HEADS[1].sql, uses: 1, bind: '.bind(a.assignmentId,a.companyId,a.userId,a.role,a.branchId,a.departmentId,now,now,',
  },
  {
    file: 'adapters/executive-report-sources.ts', helper: 'scopeBranchExistsSql',
    call: "scopeBranchExistsSql(organizationSources.executive_report_sources_organization_nodes_b_from,'g')",
    head: HEADS[2].sql, uses: 1, bind: 'guard33(key: string, value: "transfer-target"',
  },
  {
    file: 'adapters/workflow-command-authority.ts', helper: 'scopeBranchExistsSql',
    call: "scopeBranchExistsSql(organizationSources.workflow_command_authority_organization_nodes_b_from,'a')",
    head: HEADS[3].sql, uses: 1, bind: '[actor.assignmentId,actor.authority.assignmentVersion,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]',
  },
  {
    file: 'adapters/record-source-authority.ts', helper: 'hrSeatExistsSql',
    call: 'hrSeatExistsSql(organization.iam_store_authority_seats_s_from,people.iam_store_hr_people_p_join,people.iam_store_hr_engagements_e_join)',
    head: HEADS[4].sql, uses: 1, bind: 'args.push(actor.companyId,actor.companyId,iam.hrAppointmentVersion,actor.userId)',
  },
  {
    file: 'adapters/queued-social-source-authority.ts', helper: 'hrSeatExistsSql',
    call: 'hrSeatExistsSql(organization.iam_store_authority_seats_s_from,people.iam_store_hr_people_p_join,people.iam_store_hr_engagements_e_join)',
    head: HEADS[4].sql, uses: 1, bind: 's.version=${number(`${iam}->>\'hrAppointmentVersion\'`)}',
  },
  {
    file: 'adapters/iam-service.ts', helper: 'iamRoleAllowsAliasSql',
    call: "iamRoleAllowsAliasSql('allowed')",
    head: HEADS[5].sql, uses: 1, bind: 'original.role,\n                actor.companyId,\n                iamPermission(actor, required),',
  },
  {
    file: 'adapters/iam-transactions.ts', helper: 'iamRoleAllowsAliasSql',
    call: "iamRoleAllowsAliasSql('ok')",
    head: HEADS[6].sql, uses: 1, bind: 'role,\n          companyId,\n          permission,',
  },
] as const;

describe('Identity access read-tail r35 byte oracles', () => {
  test('all seven pinned heads carry the recorded SHA-256 and render byte-identically', () => {
    for (const h of HEADS) expect(sha256(h.sql), h.name).toBe(h.sha256);
    const rendered = [
      scopeBranchExistsSql('${organizationSources.authorization_organization_nodes_b_from}', 'a-multiline'),
      scopeBranchExistsSql('${organizationSources.executive_authority_source_organization_nodes_b_from}', 'grant'),
      scopeBranchExistsSql('${organizationSources.executive_report_sources_organization_nodes_b_from}', 'g'),
      scopeBranchExistsSql('${organizationSources.workflow_command_authority_organization_nodes_b_from}', 'a'),
      hrSeatExistsSql(SEATS, PJOIN, EJOIN),
      iamRoleAllowsAliasSql('allowed'),
      iamRoleAllowsAliasSql('ok'),
    ];
    rendered.forEach((sql, i) => {
      expect(sql, HEADS[i].name).toBe(HEADS[i].sql);
      expect(sha256(sql), HEADS[i].name).toBe(HEADS[i].sha256);
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    });
  });

  test('closed unions preserve company aliases, whitespace and bind order', () => {
    expect(scopeBranchExistsSql('F', 'a')).toBe(`SELECT 1 F WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active'`);
    expect(scopeBranchExistsSql('F', 'a-multiline')).toBe(`SELECT 1 F\n                    WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active'`);
    expect(scopeBranchExistsSql('F', 'grant')).toContain('b.company_id=grant_row.company_id');
    expect(scopeBranchExistsSql('F', 'g')).toContain('b.company_id=g.company_id');
    expect(hrSeatExistsSql('S', 'P', 'E')).toBe(`SELECT 1 S P ON p.id=s.person_id AND p.company_id=s.company_id AND p.status='active' AND p.merged_into_id IS NULL E ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active'`);
    expect(iamRoleAllowsAliasSql('allowed')).toBe('SELECT iam_role_allows($1,$2,$3) allowed');
    expect(iamRoleAllowsAliasSql('ok')).toBe('SELECT iam_role_allows($1,$2,$3) ok');
  });
});

describe('Identity access read-tail r35 wiring', () => {
  test('one helper per family; all eight sites route through it and keep their tails and binds', () => {
    const helper = read('adapters/read-tail-family-r35.ts');
    for (const name of ['scopeBranchExistsSql', 'hrSeatExistsSql', 'iamRoleAllowsAliasSql']) {
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
      const [file, helperName] = key.split('|');
      expect(calls(file, helperName), `${file} ${helperName}`).toBe(uses + 1); // import + uses
    }
  });

  test('locks, session authority, native kinds and r1-r34 helpers stay untouched', () => {
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/read-tail-family-r34.ts')).toBe('95e25ead2dac0a1832535f73794691036025e14748f1c17ac342c9620b3750be');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT session_id FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/iam-transactions.ts')).toContain('FOR SHARE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });

  test('post-dedupe files match their pinned byte oracles', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r35.ts', '8067ca1ffbb2a9076808a97275b1659b592eafbc2b80915e1260ffd5e2f8d442'],
      ['adapters/account-authority-snapshot.ts', '0285949693da5f29be8c591ab74f32231769fd06b3a774d54eaefe5b5106e72b'],
      ['adapters/executive-authority-source.ts', 'd6526a6b37bdcaa1b17eb55e319bf936b5431d46e8ebaa8b142adcc57baa6170'],
      ['adapters/executive-report-sources.ts', 'b815a8d4913c62d311af1f876f946e322e529070fa579b7a4450725dd9a63bd5'],
      ['adapters/workflow-command-authority.ts', '53f6e57083e1e11f61ffc21d55293733894582b081ff17d6bba2661835e662d8'],
      ['adapters/record-source-authority.ts', 'c49bb0b548f25381a932bd3532dfe19b63724b67b14f33035f597ac11da84251'],
      ['adapters/queued-social-source-authority.ts', 'a5379950fb6735056405839f43758dd14235fc8da7402097cb120e780a77663e'],
      ['adapters/iam-service.ts', '88976c449e7a56c77cfbfcf1775515342dbd341dd18cf9eecfe4c1637920e33c'],
      ['adapters/iam-transactions.ts', 'ba2ca1aa3d1a6ea08f20c1647e5ff0bd506e667f14b6ccd07409eb06db3174e3'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
  });
});

describe('Identity access read-tail r35 retirement check', () => {
  test('retirement:ts-statements is 454 after r34=459', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(459);
    expect(total).toBe(448);
  });
});
