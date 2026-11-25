import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  auditAggSql,
  branchScopeClauseSql,
  companyAuthorityProbeSql,
  scopeLabel,
} from '../../../packages/server-identity_access/src/adapters/read-tail-family-r38';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Verbatim pre-r38 heads captured from HEAD. SHA-256 values are byte-identity
// oracles only, never live query sources. The ${...} constants below are literal
// interpolation text carried into the helper arguments, not real interpolations.
const N_FROM = '${organizationSources.executive_metric_projections_organization_nodes_n_from}';
const D_FROM = '${organizationSources.executive_metric_projections_organization_nodes_d_from}';
const D_JOIN = '${organizationSources.executive_metric_projections_organization_nodes_b_join}';
const FROM = '${organizationSources.executive_metric_projections_organization_nodes_from}';
const S = { nFrom: N_FROM, dFrom: D_FROM, dJoin: D_JOIN, from: FROM };

const CA_HEAD = 'SELECT 1 FROM assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,?) AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)';
const CA_TAIL = "AND scope_kind='holding' AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?";

const HEADS = [
  {
    name: 'company-authority-read',
    sql: `${CA_HEAD}\n    ${CA_TAIL}`,
    sha256: 'fceae5d5885e06d4bc1c7803cac185df9a0c32f68c7b037c2ff332f9e60fc24b',
  },
  {
    name: 'company-authority-guard',
    sql: `${CA_HEAD} ${CA_TAIL}`,
    sha256: '4d8740287ee089126ad095e99ac53249d3899977a43781fdec4e99d7a93cba52',
  },
  {
    name: 'branch-label-x',
    sql: `CASE WHEN x.scope_kind='holding' THEN 'Holding scope' WHEN x.scope_kind='branch' THEN COALESCE((SELECT n.display_name ${N_FROM} WHERE n.id=x.scope_node_id AND n.company_id=x.company_id),'Branch') ELSE COALESCE((SELECT b.display_name ${D_FROM} ${D_JOIN} ON b.id=d.branch_id AND b.company_id=d.company_id WHERE d.id=x.scope_node_id AND d.company_id=x.company_id),'Unassigned') END`,
    sha256: '2168cfc218e1de9cd0fb60146458ea25f65db5ea0eb849eed75422fbda47838e',
  },
  {
    name: 'branch-label-i',
    sql: `CASE WHEN i.scope_kind='holding' THEN 'Holding scope' WHEN i.scope_kind='branch' THEN COALESCE((SELECT n.display_name ${N_FROM} WHERE n.id=i.scope_node_id AND n.company_id=i.company_id),'Branch') ELSE COALESCE((SELECT b.display_name ${D_FROM} ${D_JOIN} ON b.id=d.branch_id AND b.company_id=d.company_id WHERE d.id=i.scope_node_id AND d.company_id=i.company_id),'Unassigned') END`,
    sha256: 'fa03ad6f7c6ef6861f893788008eeb896b3f8e1b70c3ebe557ca2979d7d6d1c0',
  },
  {
    name: 'department-label-x',
    sql: `CASE WHEN x.scope_kind='department' THEN COALESCE((SELECT display_name ${FROM} WHERE id=x.scope_node_id AND company_id=x.company_id),'Department') ELSE 'Other scope' END`,
    sha256: '16d502695c4d35f828aa4212e747b120dac14dae28161818896a27d878b4d1b9',
  },
  {
    name: 'department-label-i',
    sql: `CASE WHEN i.scope_kind='department' THEN COALESCE((SELECT display_name ${FROM} WHERE id=i.scope_node_id AND company_id=i.company_id),'Department') ELSE 'Other scope' END`,
    sha256: '33b1d8c4472d6783fd05de5b7838b1fbee9bd50a94f53f863381785759df61a9',
  },
  {
    name: 'branch-scope-x',
    sql: `AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=?) OR (x.scope_kind='department' AND x.scope_node_id IN (SELECT id ${FROM} WHERE company_id=x.company_id AND branch_id=?)))`,
    sha256: 'a6dcdf6184ecf61b52102d2333db76a6bca9f948343304002b601915f9bcd69c',
  },
  {
    name: 'branch-scope-i',
    sql: `AND (i.scope_kind='holding' OR (i.scope_kind='branch' AND i.scope_node_id=?) OR (i.scope_kind='department' AND i.scope_node_id IN (SELECT id ${FROM} WHERE company_id=i.company_id AND branch_id=?)))`,
    sha256: '12fdcc8e6fade8d0c7879baf48d9e379d7258f603b8824c11203cfbead66890c',
  },
  {
    name: 'audit-agg-review',
    sql: '(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.created_at,event.id) FROM iam_audit event WHERE event.company_id=review.company_id AND event.target_id=review.id)',
    sha256: '0262b5d54aebeb51209ae7431717bfa26d8aebd2eea8c061e148c014ed65c19b',
  },
  {
    name: 'audit-agg-migration',
    sql: '(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.created_at,event.id) FROM iam_audit event WHERE event.company_id=migration.company_id AND event.target_id=migration.id)',
    sha256: 'b375dfae6363cec73698a4d37176433203a64a87b7b157f5a58397bc5aab40ab',
  },
] as const;

// Full rendered statements captured from HEAD, reconstructed from the same
// fragment oracles so a shape regression in any wrapper is also caught.
const FULL = [
  {
    name: 'ops-read-company-authority-effect',
    sql: `${HEADS[0].sql} LIMIT 1`,
    sha256: 'e4185f3f5f1e87a3c31a13578351b3d9336493022df39ab27d3f94ab8569d522',
  },
  {
    name: 'ops-guard6',
    sql: `EXISTS(${HEADS[1].sql})`,
    sha256: '19c3653539adc5d52b6ce0ab7d87882032c3d7ec48570b4bd91fa15c78445ccd',
  },
  {
    name: 'attachment-review',
    sql: `SELECT jsonb_build_object('review',to_jsonb(review),'audit',${HEADS[8].sql})::text image FROM iam_reviews review WHERE review.id=? AND review.company_id=?`,
    sha256: 'fb1ccd5e1203edd431aeef5ad9054dd655f1ea08a965bd519e5452da01269f64',
  },
  {
    name: 'attachment-migration',
    sql: `SELECT jsonb_build_object('migration',to_jsonb(migration),'connection',(SELECT to_jsonb(connection) FROM iam_connections connection WHERE connection.id=migration.connection_id AND connection.company_id=migration.company_id),\n 'previews',(SELECT jsonb_agg(to_jsonb(preview) ORDER BY preview.version) FROM iam_connection_previews preview WHERE preview.connection_id=migration.connection_id),\n 'mappings',(SELECT jsonb_agg(to_jsonb(mapping) ORDER BY mapping.id) FROM iam_mappings mapping WHERE mapping.connection_id=migration.connection_id AND mapping.company_id=migration.company_id),\n 'audit',${HEADS[9].sql})::text image FROM iam_migrations migration WHERE migration.id=? AND migration.company_id=?`,
    sha256: '34a76a9f4d46c9eb7979afbc21f16168141efa16d81b40445d316bc90be76ca3',
  },
] as const;

const rendered = () => [
  companyAuthorityProbeSql('read'),
  companyAuthorityProbeSql('guard'),
  scopeLabel('x', S).branch,
  scopeLabel('i', S).branch,
  scopeLabel('x', S).department,
  scopeLabel('i', S).department,
  branchScopeClauseSql('x', FROM),
  branchScopeClauseSql('i', FROM),
  auditAggSql('review'),
  auditAggSql('migration'),
];

// Expand r38 helper calls in the live source into their literal text so the
// original database.prepare/assertion statements can be recovered verbatim.
const expand = (src: string) => src
  .replace("${companyAuthorityProbeSql('read')}", companyAuthorityProbeSql('read'))
  .replace("${companyAuthorityProbeSql('guard')}", companyAuthorityProbeSql('guard'))
  .replace("${auditAggSql('review')}", auditAggSql('review'))
  .replace("${auditAggSql('migration')}", auditAggSql('migration'));

const prepared = (src: string) => [...src.matchAll(/database\.prepare\(`([\s\S]*?)`\)/g)].map((m) => m[1]);

// The executive projection keeps its status/default labels inline; expand only
// the extracted scope/branch fragments back into their pre-dedupe literal text.
const expandExec = (src: string) => {
  let b = 0;
  let d = 0;
  const branch = [scopeLabel('x', S).branch, scopeLabel('i', S).branch];
  const department = [scopeLabel('x', S).department, scopeLabel('i', S).department];
  return src
    .replace(/scope\.branch/g, () => branch[b++])
    .replace(/scope\.department/g, () => department[d++])
    .replace("branchScopeClauseSql('x', organizationSources.executive_metric_projections_organization_nodes_from)", () => branchScopeClauseSql('x', FROM))
    .replace("branchScopeClauseSql('i', organizationSources.executive_metric_projections_organization_nodes_from)", () => branchScopeClauseSql('i', FROM));
};

describe('Identity access read-tail r38 byte oracles', () => {
  test('all ten pinned heads carry the recorded SHA-256 and render byte-identically', () => {
    for (const h of HEADS) expect(sha256(h.sql), h.name).toBe(h.sha256);
    const live = rendered();
    live.forEach((sql, i) => {
      expect(sql, HEADS[i].name).toBe(HEADS[i].sql);
      expect(sha256(sql), HEADS[i].name).toBe(HEADS[i].sha256);
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    });
  });

  test('closed unions preserve alias, whitespace, wrapper and bind order', () => {
    const [readSql, guardSql] = rendered();
    expect(readSql).toBe(`${CA_HEAD}\n    ${CA_TAIL}`);
    expect(guardSql).toBe(`${CA_HEAD} ${CA_TAIL}`);
    expect(readSql).not.toBe(guardSql);
    expect(scopeLabel('x', S).branch).toContain('x.scope_node_id');
    expect(scopeLabel('i', S).branch).toContain('i.scope_node_id');
    expect(scopeLabel('x', S).branch).not.toContain('i.scope_node_id');
    expect(branchScopeClauseSql('x', FROM)).toContain('company_id=x.company_id');
    expect(branchScopeClauseSql('i', FROM)).toContain('company_id=i.company_id');
    expect(auditAggSql('review')).toContain('event.target_id=review.id');
    expect(auditAggSql('migration')).toContain('event.target_id=migration.id');
  });

  test('full pre-dedupe statements recover byte-identically from live sources', () => {
    for (const f of FULL) expect(sha256(f.sql), f.name).toBe(f.sha256);
    const ops = expand(read('adapters/operations-staff-policy-facts.ts'));
    const opsStatements = [
      prepared(ops).find((s) => s.startsWith('SELECT 1 FROM assignments WHERE id=?')),
      ...[...ops.matchAll(/assertion\(database, key, value, `([^`]*)`/g)].map((m) => m[1]),
    ];
    expect(sha256(opsStatements[0] ?? ''), FULL[0].name).toBe(FULL[0].sha256);
    expect(sha256(opsStatements[1] ?? ''), FULL[1].name).toBe(FULL[1].sha256);

    const att = prepared(expand(read('adapters/attachment-read-sources.ts')));
    expect(sha256(att[0] ?? ''), FULL[2].name).toBe(FULL[2].sha256);
    expect(sha256(att[1] ?? ''), FULL[3].name).toBe(FULL[3].sha256);

    const exp = expandExec(read('adapters/executive-metric-projections.ts'));
    for (const fragment of rendered().slice(2, 8)) expect(exp.includes(fragment), 'executive fragment survives expansion').toBe(true);
  });
});

describe('Identity access read-tail r38 wiring', () => {
  test('one helper module owns six statement tokens; all five sites route through it', () => {
    const helper = read('adapters/read-tail-family-r38.ts');
    for (const name of ['companyAuthorityProbeSql', 'scopeLabel', 'branchScopeClauseSql', 'auditAggSql']) {
      expect((helper.match(new RegExp(`export const ${name} =`, 'g')) ?? []).length, name).toBe(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(6);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    expect(calls('adapters/operations-staff-policy-facts.ts', 'companyAuthorityProbeSql')).toBe(3);
    expect(calls('adapters/executive-metric-projections.ts', 'scopeLabel')).toBe(3);
    expect(calls('adapters/executive-metric-projections.ts', 'branchScopeClauseSql')).toBe(3);
    expect(calls('adapters/attachment-read-sources.ts', 'auditAggSql')).toBe(3);

    const ops = read('adapters/operations-staff-policy-facts.ts');
    expect(ops).toContain("companyAuthorityProbeSql('read')} LIMIT 1`");
    expect(ops).toContain("EXISTS(${companyAuthorityProbeSql('guard')})`");
    expect(ops).toContain('.bind(assignmentId, companyId, value, now, now2)');
    expect(ops).toContain('[assignmentId, companyId, value2, now, now2]');
    expect(ops.includes(HEADS[0].sql)).toBe(false);
    expect(ops.includes(HEADS[1].sql)).toBe(false);

    const exec = read('adapters/executive-metric-projections.ts');
    expect(exec).toContain("scopeLabel('x', {");
    expect(exec).toContain("scopeLabel('i', {");
    expect(exec).toContain("branchScopeClauseSql('x', organizationSources.executive_metric_projections_organization_nodes_from)");
    expect(exec).toContain("branchScopeClauseSql('i', organizationSources.executive_metric_projections_organization_nodes_from)");
    expect(exec).toContain('values = [a.companyId, now, now, now, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])]');
    expect(exec).toContain('values = [a.companyId, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])]');
    for (const fragment of rendered().slice(2, 8)) expect(exec.includes(fragment), 'no inlined executive fragment').toBe(false);

    const att = read('adapters/attachment-read-sources.ts');
    expect(att).toContain("'audit',${auditAggSql('review')})::text");
    expect(att).toContain("'audit',${auditAggSql('migration')})::text");
    expect(att).toContain(".bind(id,actor.companyId).first<string>('image')");
    expect(att.includes(HEADS[8].sql)).toBe(false);
    expect(att.includes(HEADS[9].sql)).toBe(false);
  });

  test('locks, authority evaluation, native routes and r1-r36 helpers stay untouched', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r38.ts', '911541b2aea24d91a2a145ff04310048ce8bc66300e3814b333ae151f127b8e2'],
      ['adapters/read-tail-family-r29.ts', '2c392291ac6bfa7558a022f750fba0742acfe7014702875d8836321063650775'],
      ['adapters/read-tail-family-r34.ts', '95e25ead2dac0a1832535f73794691036025e14748f1c17ac342c9620b3750be'],
      ['adapters/read-tail-family-r35.ts', '8067ca1ffbb2a9076808a97275b1659b592eafbc2b80915e1260ffd5e2f8d442'],
      ['adapters/read-tail-family-r36.ts', 'c2d4f046bff69a0fc400d1bc07e5361f69d2fbe9a45c98c6ec8aa8b70485d9aa'],
      ['adapters/iam-probe-sql.ts', 'e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101'],
      ['adapters/payroll-cancellation-grant.ts', 'eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee'],
      ['adapters/native-session.ts', '1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1'],
      ['adapters/product-sso-store.ts', '967b754535bb65e0196bf47fa0c0292c65b30276291e137c926d22579a3f3441'],
      ['adapters/workflow-command-authority.ts', '53f6e57083e1e11f61ffc21d55293733894582b081ff17d6bba2661835e662d8'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
    expect(read('adapters/iam-probe-sql.ts')).toContain('SELECT 1 FROM iam_revoked_sessions WHERE session_id=${sessionParam}');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE iam_reviews,iam_migrations,iam_connections,iam_connection_previews,iam_mappings,iam_audit IN SHARE ROW EXCLUSIVE MODE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });

  test('post-dedupe files match their pinned byte oracles', () => {
    const pins: [string, string][] = [
      ['adapters/operations-staff-policy-facts.ts', '8609823a7c3ba90fe9a37ab65608da7ad836379dc122df42735ccb73716b9721'],
      ['adapters/executive-metric-projections.ts', '7150522f15483cc80fcfbbbcddf91a7f071e329d733a1eca76c642478d6c5374'],
      ['adapters/attachment-read-sources.ts', '3cedcaf49a89f99df88a5fbe324b20046c2aeb2cd1a298e9b300039b6b38b3ab'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
  });
});

describe('Identity access read-tail r38 retirement check', () => {
  test('retirement:ts-statements is 442 after r36=448', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(448);
    expect(total).toBe(442);
  });
});
