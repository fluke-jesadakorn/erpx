import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { execMetricLabelHead } from '../../../packages/server-identity_access/src/adapters/read-tail-family-r40';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim pre-r40 head and statements captured from HEAD. The ${...} pieces
// below are literal interpolation text from the original template literals.
// These SHA-256 values are byte-identity oracles only, never live query sources.
const HEAD = 'SELECT ${label} label,count(*)::text value FROM ';
const TAIL_X = 'assignments x WHERE x.company_id=? AND x.revoked_at IS NULL AND x.valid_from<=? AND x.valid_until>? AND iam_assignment_active(x.id,?) ${scopeClause} GROUP BY 1 ORDER BY 1';
const TAIL_I = "iam_access_review_items i JOIN iam_access_review_campaigns c ON c.id=i.campaign_id AND c.company_id=i.company_id WHERE i.company_id=? AND i.decision='pending' AND c.status='open' ${scopeClause} GROUP BY 1 ORDER BY 1";
const FULL_X = HEAD + TAIL_X;
const FULL_I = HEAD + TAIL_I;
const RENDER_X = 'SELECT x.scope_kind label,count(*)::text value FROM ';
const RENDER_I = "SELECT 'Pending reviews' label,count(*)::text value FROM ";

const FULLS = [
  { name: 'active-assignments', sql: FULL_X, sha256: 'f55286e13cad77d6d921b23b01e448857e765726a280a7203ab5ce28ed03fd5b' },
  { name: 'pending-access-reviews', sql: FULL_I, sha256: 'f8dc20960416434f7cc30618929a6d5b85279ae1d6b3ba165a381b967480d201' },
] as const;

const rendered = () => [execMetricLabelHead('x.scope_kind'), execMetricLabelHead("'Pending reviews'")];

// Expand the r40 helper calls in the live source back into their literal head
// so the original sql template statements are recovered verbatim.
const expand = (src: string) => src.replace(/\$\{execMetricLabelHead\(label\)\}/g, () => HEAD);
const prepared = (src: string) => [...src.matchAll(/sql = `([^`]*)`;/g)].map((m) => m[1]);

describe('Identity access read-tail r40 byte oracles', () => {
  test('the pinned head and both full statements carry their recorded SHA-256', () => {
    expect(sha256(HEAD)).toBe('afe0a693571b1948793c62739761e5c34abd8fc5a9152d454ef8fbd59e2bdc42');
    for (const f of FULLS) expect(sha256(f.sql), f.name).toBe(f.sha256);
    expect(FULL_X.startsWith(HEAD)).toBe(true);
    expect(FULL_I.startsWith(HEAD)).toBe(true);
    expect(FULL_X).not.toBe(FULL_I);
  });

  test('the renderer emits the retired head bytes verbatim for both label shapes', () => {
    expect(sha256(RENDER_X)).toBe('455fceba6c4ec25036a4ef8de9edbc6b3f47ab27cd1dd1ad8d5ec47a5d0b8b14');
    expect(sha256(RENDER_I)).toBe('b3502447b95240c3de3abefbd8b80b72d133fe13a1357c7fe3a1990bef5b73af');
    const live = rendered();
    expect(live[0]).toBe(RENDER_X);
    expect(live[1]).toBe(RENDER_I);
    live.forEach((sql) => {
      expect(sql).toMatch(/^SELECT .+ label,count\(\*\)::text value FROM $/);
      expect(sql.includes('${')).toBe(false);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    });
  });

  test('both pre-dedupe statements recover byte-identically from the live source', () => {
    const recovered = prepared(expand(read('adapters/executive-metric-projections.ts')));
    expect(recovered).toHaveLength(2);
    recovered.forEach((sql, i) => {
      expect(sql, FULLS[i].name).toBe(FULLS[i].sql);
      expect(sha256(sql), FULLS[i].name).toBe(FULLS[i].sha256);
    });
  });
});

describe('Identity access read-tail r40 wiring', () => {
  test('one owner-local head helper owns the statement token; both sites route through it', () => {
    const helper = read('adapters/read-tail-family-r40.ts');
    expect((helper.match(/export const execMetricLabelHead =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const exec = read('adapters/executive-metric-projections.ts');
    expect((exec.match(/\bexecMetricLabelHead\(/g) ?? []).length).toBe(2);
    expect(exec).toContain("import { execMetricLabelHead } from './read-tail-family-r40';");
    expect(exec).toContain('${execMetricLabelHead(label)}assignments x WHERE');
    expect(exec).toContain('${execMetricLabelHead(label)}iam_access_review_items i JOIN');
    expect(exec.includes('SELECT ${label} label,count(*)::text value FROM')).toBe(false);
    expect(exec).toContain('values = [a.companyId, now, now, now, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])]');
    expect(exec).toContain('values = [a.companyId, ...(a.departmentId ? [a.branchId, a.departmentId] : a.branchId ? [a.branchId, a.branchId] : [])]');
  });

  test('authority evaluation, locks, native routes, public contracts and r38/r39 helpers stay untouched', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r40.ts', 'c11d55c10680737237eaff32c11b3eddf074384046a5cdf66becad330a811690'],
      ['adapters/read-tail-family-r38.ts', '911541b2aea24d91a2a145ff04310048ce8bc66300e3814b333ae151f127b8e2'],
      ['adapters/read-tail-family-r39.ts', '47f1dcbc1b807f6f2871e2eb1ade3be5997b3f43793a71c2f2532f71aef01397'],
      ['adapters/attachment-read-sources.ts', '3cedcaf49a89f99df88a5fbe324b20046c2aeb2cd1a298e9b300039b6b38b3ab'],
      ['adapters/operations-staff-policy-facts.ts', '8609823a7c3ba90fe9a37ab65608da7ad836379dc122df42735ccb73716b9721'],
      ['adapters/people-lifecycle-facts.ts', '6526902571aef0fd32fd3c6c0237660b5bf2bf313adb12dc2b6b4bbcdbc31d37'],
      ['adapters/iam-probe-sql.ts', 'e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101'],
      ['adapters/payroll-cancellation-grant.ts', 'eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee'],
      ['adapters/native-session.ts', '1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1'],
      ['adapters/product-sso-store.ts', '967b754535bb65e0196bf47fa0c0292c65b30276291e137c926d22579a3f3441'],
      ['adapters/workflow-command-authority.ts', '53f6e57083e1e11f61ffc21d55293733894582b081ff17d6bba2661835e662d8'],
      ['adapters/executive-metric-projections.ts', 'b93aa61f9ea991ca199fe69131e013fbc272f90aaddc6ec8f37eb3566c3644de'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
    expect(read('adapters/iam-probe-sql.ts')).toContain('SELECT 1 FROM iam_revoked_sessions WHERE session_id=${sessionParam}');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE iam_reviews,iam_migrations,iam_connections,iam_connection_previews,iam_mappings,iam_audit IN SHARE ROW EXCLUSIVE MODE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access read-tail r40 retirement check', () => {
  test('retirement:ts-statements is 373 after lock-aware preference cut', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(440);
    expect(total).toBe(373);
  });
});
