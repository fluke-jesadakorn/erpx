import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { verifiedLifecycleAccountSql } from '../../../packages/server-identity_access/src/adapters/read-tail-family-r39';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Verbatim pre-r39 heads captured from HEAD. These SHA-256 values are
// byte-identity oracles only, never live query sources. The ${...} fragments
// in FULL are literal interpolation text carried into the rendered statement,
// not real interpolations.
const PLAIN = `SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.company_id=c.company_id AND m.person_id=c.person_id AND m.status='active'`;
const TAIL = " AND NOT EXISTS(SELECT 1 FROM people_directory_operations o WHERE o.company_id=c.company_id AND o.connection_id=i.connection_id AND o.status='completed' AND o.result->>'verified'='true' AND o.definition->'action'->>'kind'='account.disable' AND o.definition->'action'->>'caseId'=c.id AND o.definition->'action'->>'objectId'=i.external_id)";
const PROBE = `${PLAIN}${TAIL}`;

const HEADS = [
  { name: 'verified-lifecycle-account-plain', sql: PLAIN, sha256: '95090f098ca212563efd37aaa0da62cf4361ceb9a987d791dcb32d654da53b53' },
  { name: 'verified-lifecycle-account-disable-probe', sql: PROBE, sha256: '41cc40fa6b03f2e4678134252fa07ac8bb3b7be2562e87a67198ef6551cd3acc' },
] as const;

// Full prepared statement captured from HEAD, reconstructed from the same
// head oracles so a shape regression in the surrounding CASE/prepare wrapper
// is also caught.
const FULL = `SELECT \${caseColumns},CASE WHEN NOT EXISTS(${PLAIN}) THEN 'not_required' WHEN NOT EXISTS(${PROBE}) THEN 'verified' ELSE 'pending' END AS "directoryStatus" \${peoplepayrollSources.people_lifecycle_facts_people_lifecycle_cases_c_from} \${peoplepayrollSources.people_lifecycle_facts_hr_engagements_e_join} ON e.id=c.engagement_id AND e.company_id=c.company_id WHERE c.company_id=? AND e.worker_type='employee' AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY c.updated_at DESC,c.id`;
const FULL_SHA = '663f161a6654d056e56a1e4a1bbddb21cb875bf0de3f90e003717f204e5e5c50';

const rendered = () => [verifiedLifecycleAccountSql('plain'), verifiedLifecycleAccountSql('disable-probe')];

// Expand r39 helper calls in the live source back into their literal text so
// the original database.prepare statement is recovered verbatim.
const expand = (src: string) => src
  .replace("${verifiedLifecycleAccountSql('plain')}", verifiedLifecycleAccountSql('plain'))
  .replace("${verifiedLifecycleAccountSql('disable-probe')}", verifiedLifecycleAccountSql('disable-probe'));

const prepared = (src: string) => [...src.matchAll(/database\.prepare\(`([\s\S]*?)`\)/g)].map((m) => m[1]);

describe('Identity access read-tail r39 byte oracles', () => {
  test('both pinned heads carry the recorded SHA-256 and render byte-identically', () => {
    for (const h of HEADS) expect(sha256(h.sql), h.name).toBe(h.sha256);
    const live = rendered();
    live.forEach((sql, i) => {
      expect(sql, HEADS[i].name).toBe(HEADS[i].sql);
      expect(sha256(sql), HEADS[i].name).toBe(HEADS[i].sha256);
      expect(sql).toContain('SELECT 1 FROM iam_identities i JOIN iam_members m');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|iam_role_allows|iam_assignment_active|iam_principal_active/i);
    });
  });

  test('closed union preserves the plain/disable-probe gap and bind-free head', () => {
    const [plain, probe] = rendered();
    expect(probe.startsWith(plain)).toBe(true);
    expect(probe.slice(plain.length)).toBe(TAIL);
    expect(plain).not.toBe(probe);
    expect((plain.match(/SELECT /g) ?? []).length).toBe(1);
    expect((probe.match(/SELECT /g) ?? []).length).toBe(2);
    expect(plain).toContain("m.person_id=c.person_id AND m.status='active'");
    expect(probe).toContain("o.definition->'action'->>'objectId'=i.external_id)");
  });

  test('full pre-dedupe statement recovers byte-identically from the live source', () => {
    expect(sha256(FULL)).toBe(FULL_SHA);
    const stmt = prepared(expand(read('adapters/people-lifecycle-facts.ts')))[0] ?? '';
    expect(sha256(stmt)).toBe(FULL_SHA);
    expect(stmt).toContain(PLAIN);
    expect(stmt).toContain(PROBE);
    expect(read('adapters/people-lifecycle-facts.ts')).toContain('.bind(companyId, branchId, branchId2, departmentId, departmentId2)');
  });
});

describe('Identity access read-tail r39 wiring', () => {
  test('one owner-local helper owns both heads; both sites route through it', () => {
    const helper = read('adapters/read-tail-family-r39.ts');
    expect((helper.match(/export const verifiedLifecycleAccountSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(2);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|iam_role_allows|iam_assignment_active|iam_principal_active/i);

    const plf = read('adapters/people-lifecycle-facts.ts');
    // Count call sites only (import line has no '('); the plain `calls` helper
    // would include the import and incorrectly report 3.
    expect((plf.match(/\bverifiedLifecycleAccountSql\(/g) ?? []).length).toBe(2);
    expect(plf).toContain("import { verifiedLifecycleAccountSql } from './read-tail-family-r39';");
    expect(plf).toContain("NOT EXISTS(${verifiedLifecycleAccountSql('plain')})");
    expect(plf).toContain("NOT EXISTS(${verifiedLifecycleAccountSql('disable-probe')})");
    expect(plf.includes(PLAIN)).toBe(false);
    expect(plf.includes(PROBE)).toBe(false);
  });

  test('locks, authority evaluation, native routes and r1-r38 helpers stay untouched', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r39.ts', '47f1dcbc1b807f6f2871e2eb1ade3be5997b3f43793a71c2f2532f71aef01397'],
      ['adapters/people-lifecycle-facts.ts', '6526902571aef0fd32fd3c6c0237660b5bf2bf313adb12dc2b6b4bbcdbc31d37'],
      ['adapters/read-tail-family-r38.ts', '911541b2aea24d91a2a145ff04310048ce8bc66300e3814b333ae151f127b8e2'],
      ['adapters/read-tail-family-r29.ts', '2c392291ac6bfa7558a022f750fba0742acfe7014702875d8836321063650775'],
      ['adapters/read-tail-family-r34.ts', '95e25ead2dac0a1832535f73794691036025e14748f1c17ac342c9620b3750be'],
      ['adapters/read-tail-family-r35.ts', '8067ca1ffbb2a9076808a97275b1659b592eafbc2b80915e1260ffd5e2f8d442'],
      ['adapters/read-tail-family-r36.ts', 'c2d4f046bff69a0fc400d1bc07e5361f69d2fbe9a45c98c6ec8aa8b70485d9aa'],
      ['adapters/iam-probe-sql.ts', 'e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101'],
      ['adapters/payroll-cancellation-grant.ts', 'eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee'],
      ['adapters/native-session.ts', '1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1'],
      ['adapters/product-sso-store.ts', '967b754535bb65e0196bf47fa0c0292c65b30276291e137c926d22579a3f3441'],
      ['adapters/workflow-command-authority.ts', 'd0939ee195594843eb159709f7214996667b7174d418235750e4ca09c93268ca'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
    expect(read('adapters/iam-probe-sql.ts')).toContain('SELECT 1 FROM iam_revoked_sessions WHERE session_id=${sessionParam}');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE iam_reviews,iam_migrations,iam_connections,iam_connection_previews,iam_mappings,iam_audit IN SHARE ROW EXCLUSIVE MODE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access read-tail r39 retirement check', () => {
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
    expect(total).toBeLessThan(442);
    expect(total).toBe(368);
  });
});
