import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { jsonbArrayHeadSql } from '../../../packages/server-identity_access/src/adapters/read-tail-family-r42';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const placeholders = (s: string) => (s.match(/\$\d+|\?/g) ?? []).length;
const calls = (rel: string) => (read(rel).match(/\bjsonbArrayHeadSql\(/g) ?? []).length;

// Verbatim pre-r42 head and sub-statements captured from HEAD. The ${...} shape
// is the literal interpolation text carried through the helper call. These
// SHA-256 values are byte-identity oracles only, never live query sources.
const HEAD = 'SELECT 1 FROM jsonb_array_elements(';
const RULE_EXPR = "definition->'rule'->'conditions'";
const DECISIONS_EXPR = "COALESCE(r.approval_json::jsonb -> 'decisions','[]')";
const PTA_STATEMENT =
  "SELECT 1 FROM um_groups WHERE company_id=$1 AND NOT (definition->>'archived')::boolean AND EXISTS(" +
  HEAD + RULE_EXPR + ") x WHERE x->>'field'=$2)";
const REC_FRAGMENT = HEAD + DECISIONS_EXPR + ") decision WHERE decision ->> 'userId'=?";

const ORACLES = [
  { name: 'jsonb array EXISTS head', sql: HEAD, bytes: 35, binds: 0, sha: '45f20dc72edb4a1dcd9cdd78f3a6d6b982aa3db3937e84033636ac18d286dc22' },
  { name: 'people-template groupReferences statement', sql: PTA_STATEMENT, bytes: 190, binds: 2, sha: '5ae2987e3ed8e9d9d67b77ac23f7fabbdb5d6b5499560f7617ad18bf9b1364e2' },
  { name: 'employee-request decisions EXISTS fragment', sql: REC_FRAGMENT, bytes: 127, binds: 1, sha: 'b9eb94f5b62326a400d2bc932bfa822789607946777317acfe625f7ee1d1ea10' },
] as const;

const RENDER_RULE = HEAD + RULE_EXPR + ')';
const RENDER_DECISIONS = HEAD + DECISIONS_EXPR + ')';

// Expand the r42 helper calls in the live source back into their retired head
// so the original statements are recovered verbatim.
const expand = (src: string) =>
  src.replace(/\$\{jsonbArrayHeadSql\("([^"]*)"\)\}/g, (_m, expr: string) => HEAD + expr + ')');
const ptaRecovered = () =>
  expand(read('adapters/people-template-attributes.ts')).match(/`(SELECT 1 FROM um_groups[^`]*)`/)?.[1] ?? '';
const recRecovered = () =>
  expand(read('adapters/records-employee-requests-facts.ts')).includes(REC_FRAGMENT) ? REC_FRAGMENT : '';

describe('Identity access read-tail r42 byte oracles', () => {
  test('the retired head and both sub-statements carry their recorded SHA-256', () => {
    expect(sha256(HEAD)).toBe('45f20dc72edb4a1dcd9cdd78f3a6d6b982aa3db3937e84033636ac18d286dc22');
    expect(PTA_STATEMENT.includes(HEAD)).toBe(true);
    expect(REC_FRAGMENT.startsWith(HEAD)).toBe(true);
    for (const o of ORACLES) {
      expect(Buffer.byteLength(o.sql), o.name).toBe(o.bytes);
      expect(placeholders(o.sql), o.name).toBe(o.binds);
      expect(sha256(o.sql), o.name).toBe(o.sha);
      expect(o.sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the renderer emits the retired head bytes verbatim for both array expressions', () => {
    const helper = read('adapters/read-tail-family-r42.ts');
    expect((helper.match(/export const jsonbArrayHeadSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(jsonbArrayHeadSql(RULE_EXPR)).toBe(RENDER_RULE);
    expect(jsonbArrayHeadSql(DECISIONS_EXPR)).toBe(RENDER_DECISIONS);
    expect(sha256(jsonbArrayHeadSql(RULE_EXPR))).toBe(sha256(HEAD + RULE_EXPR + ')'));
    expect(sha256(jsonbArrayHeadSql(DECISIONS_EXPR))).toBe(sha256(HEAD + DECISIONS_EXPR + ')'));
  });

  test('byte oracles survive lane 12 retirement of the people-template statement', () => {
    const pta = ptaRecovered();
    const rec = recRecovered();
    // The people-template statement is now executed by the native lane 12 owner;
    // TS no longer carries it, but its historical bytes stay pinned here.
    expect(pta).toBe('');
    expect(sha256(PTA_STATEMENT)).toBe(ORACLES[1].sha);
    expect(rec).toBe(REC_FRAGMENT);
    expect(sha256(rec)).toBe(ORACLES[2].sha);
  });
});

describe('Identity access read-tail r42 wiring', () => {
  test('one owner-local head helper owns the fragment; its remaining site keeps binds untouched', () => {
    const pta = read('adapters/people-template-attributes.ts');
    const rec = read('adapters/records-employee-requests-facts.ts');
    // Lane 12 retired the people-template site; its read is a native transport.
    expect(pta).not.toContain("import { jsonbArrayHeadSql } from './read-tail-family-r42';");
    expect(pta).toContain("from './native-lane-12/client'");
    expect(rec).toContain("import { jsonbArrayHeadSql } from './read-tail-family-r42';");
    expect(calls('adapters/people-template-attributes.ts')).toBe(0);
    expect(calls('adapters/records-employee-requests-facts.ts')).toBe(1);
    expect(pta.includes(HEAD)).toBe(false);
    expect(rec.includes(HEAD)).toBe(false);
    expect(rec).toContain('EXISTS(${jsonbArrayHeadSql("COALESCE(r.approval_json::jsonb -> \'decisions\',\'[]\')")} decision WHERE decision ->> \'userId\'=?)');
    expect(rec).toContain('.bind(companyId, branchId, branchId2, departmentId, departmentId2, value, userId, userId2, userId3, userId4, userId5, userId6)');
  });

  test('authority evaluation, terminal locks, native routes and public contracts stay untouched', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r42.ts', '6a871bef3a8c472ac8ba3ba66fb54732e799b55b0b82ab31bcf3e715d457a28e'],
      ['adapters/people-template-attributes.ts', 'c7fc633ced69a2d7d57d29fd445ab1c8b7a158c56ad05948adcdf3ae0854deeb'],
      ['adapters/records-employee-requests-facts.ts', '199b5bb8a36ac96ac4aafc144546692607587626b0d8ba979535a040051f90af'],
      ['adapters/read-tail-family-r41.ts', '37addf1163c7188edda85df22f73a018acae972133c15e6dc6d62c44ee060d4d'],
      ['adapters/read-tail-family-r40.ts', 'c11d55c10680737237eaff32c11b3eddf074384046a5cdf66becad330a811690'],
      ['adapters/read-tail-family-r39.ts', '47f1dcbc1b807f6f2871e2eb1ade3be5997b3f43793a71c2f2532f71aef01397'],
      ['adapters/read-tail-family-r38.ts', '911541b2aea24d91a2a145ff04310048ce8bc66300e3814b333ae151f127b8e2'],
      ['adapters/attachment-read-sources.ts', '3cedcaf49a89f99df88a5fbe324b20046c2aeb2cd1a298e9b300039b6b38b3ab'],
      ['adapters/native-session.ts', '1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1'],
      ['adapters/workflow-command-authority.ts', '53f6e57083e1e11f61ffc21d55293733894582b081ff17d6bba2661835e662d8'],
      ['public/iam-inspector.ts', '0b07fc0a4b209c5a35c2af36555f4a3943a038855c707ff8fbbecbec51c5b0ab'],
      ['public/people-lifecycle-facts.ts', 'faee779bd8df8be4a9081ced03bb3cff8aa8a47312c9e80cee1ffba6adc8d4a8'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE iam_reviews,iam_migrations,iam_connections,iam_connection_previews,iam_mappings,iam_audit IN SHARE ROW EXCLUSIVE MODE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access read-tail r42 retirement check', () => {
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
    expect(total).toBeLessThan(374);
    expect(total).toBe(373);
  });
});
