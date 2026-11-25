import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { iamMemberByCompanyPersonSql } from '../../../packages/server-identity_access/src/adapters/read-tail-family-r41';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim pre-r41 literals captured from HEAD. These SHA-256 values are
// byte-identity oracles only, never live query sources.
const FRAGMENT = 'SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?';
const INSPECTOR_STATEMENT = FRAGMENT;
const PLF_STATEMENT = `SELECT DISTINCT u.user_id FROM (SELECT user_id \${peoplepayrollSources.people_lifecycle_facts_hr_people_from} WHERE company_id=? AND id=? UNION SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?) u LEFT JOIN iam_members m ON m.company_id=? AND m.user_id=u.user_id WHERE u.user_id IS NOT NULL AND (NOT ?::boolean OR COALESCE(m.status,'active')='active') AND (?::text IS NULL OR CASE WHEN ?='local' THEN COALESCE(m.login_policy,'local')!='directory' ELSE m.login_policy='directory' AND m.connection_id=? END) ORDER BY u.user_id`;

const ORACLES = [
  { name: 'inspector aliases (2 binds)', sql: INSPECTOR_STATEMENT, bytes: 66, binds: 2, sha: '567828eb4c50dcf468f1312042cc9801f59b912f65caf72ef845b5927d4eb298' },
  { name: 'people lifecycle linked accounts (9 binds)', sql: PLF_STATEMENT, bytes: 540, binds: 9, sha: 'c02ffde1fd44f5ac291407e22f74e61ff52472f592f4be08f39b3d35f4e196db' },
] as const;

// Expand the r41 helper calls in the live source back into their retired
// literal so the original statements are recovered verbatim.
const expandInspector = (src: string) => src.replace(/iamMemberByCompanyPersonSql\(\)/g, `'${FRAGMENT}'`);
const expandPlf = (src: string) => src.replace(/\$\{iamMemberByCompanyPersonSql\(\)\}/g, FRAGMENT);
const recovered = () => [
  expandInspector(read('adapters/iam-inspector.ts')).match(/db\.prepare\((['"`])(SELECT user_id FROM iam_members[^'"`]*)\1\)/)?.[2] ?? '',
  expandPlf(read('adapters/people-lifecycle-facts.ts')).match(/database\.prepare\(`([^`]*)`\)\s*\.bind\(companyId, personId, companyId2, personId2, companyId3, activeOnly, value, value2, value3\)/)?.[1] ?? '',
];

describe('Identity access read-tail r41 byte oracles', () => {
  test('the retired fragment and both full statements carry their recorded SHA-256', () => {
    expect(sha256(FRAGMENT)).toBe('567828eb4c50dcf468f1312042cc9801f59b912f65caf72ef845b5927d4eb298');
    expect(Buffer.byteLength(FRAGMENT)).toBe(66);
    expect(PLF_STATEMENT.includes(FRAGMENT)).toBe(true);
    for (const o of ORACLES) {
      expect(Buffer.byteLength(o.sql), o.name).toBe(o.bytes);
      expect((o.sql.match(/\?/g) ?? []).length, o.name).toBe(o.binds);
      expect(sha256(o.sql), o.name).toBe(o.sha);
      expect(o.sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('the renderer emits the retired fragment verbatim and owns the single statement token', () => {
    const helper = read('adapters/read-tail-family-r41.ts');
    expect((helper.match(/export const iamMemberByCompanyPersonSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(iamMemberByCompanyPersonSql()).toBe(FRAGMENT);
    expect(sha256(iamMemberByCompanyPersonSql())).toBe(ORACLES[0].sha);
    expect((iamMemberByCompanyPersonSql().match(/\?/g) ?? []).length).toBe(2);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('both pre-dedupe statements recover byte-identically from the live source', () => {
    const live = recovered();
    expect(live).toHaveLength(2);
    live.forEach((sql, i) => {
      expect(sql, ORACLES[i].name).toBe(ORACLES[i].sql);
      expect(sha256(sql), ORACLES[i].name).toBe(ORACLES[i].sha);
    });
  });
});

describe('Identity access read-tail r41 wiring', () => {
  test('one owner-local helper owns the fragment; both sites route through it with binds untouched', () => {
    const insp = read('adapters/iam-inspector.ts');
    const plf = read('adapters/people-lifecycle-facts.ts');
    expect(insp).toContain("import {iamMemberByCompanyPersonSql} from './read-tail-family-r41';");
    expect(plf).toContain("import { iamMemberByCompanyPersonSql } from './read-tail-family-r41';");
    expect((insp.match(/iamMemberByCompanyPersonSql\(\)/g) ?? []).length).toBe(1);
    expect((plf.match(/iamMemberByCompanyPersonSql\(\)/g) ?? []).length).toBe(1);
    expect(insp.includes(FRAGMENT)).toBe(false);
    expect(plf.includes(FRAGMENT)).toBe(false);
    expect(insp).toContain('db.prepare(iamMemberByCompanyPersonSql()).bind(companyId, person)');
    expect(plf).toContain('UNION ${iamMemberByCompanyPersonSql()}) u LEFT JOIN iam_members m');
    expect(plf).toContain('.bind(companyId, personId, companyId2, personId2, companyId3, activeOnly, value, value2, value3)');
    expect(plf).toContain("${peoplepayrollSources.people_lifecycle_facts_hr_people_from} WHERE company_id=? AND id=?");
  });

  test('authority evaluation, terminal locks, native routes and public contracts stay untouched', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r41.ts', '37addf1163c7188edda85df22f73a018acae972133c15e6dc6d62c44ee060d4d'],
      ['adapters/iam-inspector.ts', 'c0042e2b5ff14b9070b9c5583ede377fecd07bae3aa2b2124ec245bbfdb04863'],
      ['adapters/people-lifecycle-facts.ts', '6526902571aef0fd32fd3c6c0237660b5bf2bf313adb12dc2b6b4bbcdbc31d37'],
      ['adapters/read-tail-family-r40.ts', 'c11d55c10680737237eaff32c11b3eddf074384046a5cdf66becad330a811690'],
      ['adapters/read-tail-family-r39.ts', '47f1dcbc1b807f6f2871e2eb1ade3be5997b3f43793a71c2f2532f71aef01397'],
      ['adapters/read-tail-family-r38.ts', '911541b2aea24d91a2a145ff04310048ce8bc66300e3814b333ae151f127b8e2'],
      ['adapters/attachment-read-sources.ts', '3cedcaf49a89f99df88a5fbe324b20046c2aeb2cd1a298e9b300039b6b38b3ab'],
      ['adapters/native-session.ts', '1c390e8f6c47b25da718120ef5f6546103b1f8bbbfa837406ff7be462d1a51c1'],
      ['adapters/workflow-command-authority.ts', 'd0939ee195594843eb159709f7214996667b7174d418235750e4ca09c93268ca'],
      ['public/iam-inspector.ts', '0b07fc0a4b209c5a35c2af36555f4a3943a038855c707ff8fbbecbec51c5b0ab'],
      ['public/people-lifecycle-facts.ts', 'faee779bd8df8be4a9081ced03bb3cff8aa8a47312c9e80cee1ffba6adc8d4a8'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE iam_reviews,iam_migrations,iam_connections,iam_connection_previews,iam_mappings,iam_audit IN SHARE ROW EXCLUSIVE MODE');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access read-tail r41 retirement check', () => {
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
    expect(total).toBeLessThan(440);
    expect(total).toBe(368);
  });
});
