import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { revokedSessionHeadSql } from '../../../packages/server-identity_access/src/adapters/read-tail-family-r36';

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Verbatim pre-r36 revocation-probe heads captured from HEAD (454 SELECT tokens).
// SHA-256 values are byte-identity oracles only, never live query sources. An
// escaped `${` is the literal interpolation text carried into the helper call.
const HEADS = [
  {
    name: 'store-cast',
    file: 'iam-store.ts',
    sql: 'SELECT 1 FROM iam_revoked_sessions WHERE session_id=?::text AND company_id=?::text AND user_id=?::text',
    sha256: 'b2ef0d71f3174b1cbeb46ff4932e45c05620411a2e5fb8f40d426012d5a5369e',
    call: "revokedSessionHeadSql('?::text','?::text','?::text','unaliased-session-first')",
    args: ['?::text', '?::text', '?::text', 'unaliased-session-first'] as const,
    bind: '.bind(companyId, id)',
  },
  {
    name: 'grant-alias',
    file: 'executive-authority-source.ts',
    sql: 'SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=grant_row.company_id AND user_id=grant_row.user_id',
    sha256: '3cbc0e72512a900fdca8f7753b6421398fcbc1221d8794fb4e84aeaf803e1e40',
    call: "revokedSessionHeadSql('?','grant_row.company_id','grant_row.user_id','unaliased-session-first')",
    args: ['?', 'grant_row.company_id', 'grant_row.user_id', 'unaliased-session-first'] as const,
    bind: '.bind(a.assignmentId,a.companyId,a.userId,a.role,a.branchId,a.departmentId,now,now,',
  },
  {
    name: 'directory-company-first',
    file: 'iam-directory-refresh.ts',
    sql: 'SELECT 1 FROM iam_revoked_sessions WHERE company_id=? AND user_id=? AND session_id=?',
    sha256: '4b7f1c2012581526b44585d5b60c1638dd3b0ce7ae283b556e8f34944a5bd784',
    call: "revokedSessionHeadSql('?','?','?','unaliased-company-first')",
    args: ['?', '?', '?', 'unaliased-company-first'] as const,
    bind: '.bind(companyId,userId,sessionId,companyId,userId,Date.now()-300000)',
  },
  {
    name: 'job-a-alias',
    file: 'project-worker-startup-subject.ts',
    sql: 'SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=a.company_id AND r.user_id=a.user_id AND r.session_id=?',
    sha256: '37d50e36714c5a1a48beecf6f618ff7132409bd1c4f0c3cf5fbdb1fd4d5bcb8a',
    call: "revokedSessionHeadSql('?','a.company_id','a.user_id','aliased-company-first')",
    args: ['?', 'a.company_id', 'a.user_id', 'aliased-company-first'] as const,
    bind: '.bind(saved.assignmentId,actorId,companyId,saved.role,iam.queuedAssignmentVersion??saved.assignmentVersion??null,saved.sessionId??null)',
  },
  {
    name: 'queued-alias',
    file: 'queued-command-authority.ts',
    sql: 'SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=${alias}.company_id AND r.user_id=? AND r.session_id=?',
    sha256: '1e9a2196d9a5be225960c3edf60866fe59039ff8a3fafc8d25da521beeca89b3',
    call: "revokedSessionHeadSql('?', `${alias}.company_id`, '?', 'aliased-company-first')",
    args: ['?', '${alias}.company_id', '?', 'aliased-company-first'] as const,
    bind: '.bind(...scope.args,actor.userId,actor.authority?.sessionId??null,actor.iam?.queuedAssignmentVersion??null,',
  },
  {
    name: 'queued-interp',
    file: 'queued-social-source-authority.ts',
    sql: 'SELECT 1 FROM iam_revoked_sessions r WHERE r.session_id=${session} AND r.company_id=${company} AND r.user_id=${user}',
    sha256: '4780fe68fb7e5b74ddc2a35ca8c7171ef49638dbc1066e228aa8c38633afd152',
    call: "revokedSessionHeadSql(`${session}`, `${company}`, `${user}`, 'aliased-session-first')",
    args: ['${session}', '${company}', '${user}', 'aliased-session-first'] as const,
    bind: "iamMemberVersionProbeSql('interp',company,user)",
  },
  {
    name: 'push-multiline',
    file: 'collaboration-push-session.ts',
    sql: 'SELECT 1 FROM iam_revoked_sessions r WHERE r.session_id=c.session_id\n      AND r.company_id=c.company_id AND r.user_id=c.user_id',
    sha256: 'dad7cfb0764b81f485eba9fc358d35556855cf0f208e35cd1283815601786665',
    call: "revokedSessionHeadSql('c.session_id','c.company_id','c.user_id','aliased-session-first-multiline')",
    args: ['c.session_id', 'c.company_id', 'c.user_id', 'aliased-session-first-multiline'] as const,
    bind: '.bind(s.session_id,s.company_id,s.user_id,now,now,now)',
  },
] as const;

describe('Identity access read-tail r36 byte oracles', () => {
  test('all seven pinned heads carry the recorded SHA-256 and render byte-identically', () => {
    for (const h of HEADS) {
      expect(sha256(h.sql), h.name).toBe(h.sha256);
      const rendered = revokedSessionHeadSql(h.args[0], h.args[1], h.args[2], h.args[3]);
      expect(rendered, h.name).toBe(h.sql);
      expect(sha256(rendered), h.name).toBe(h.sha256);
      expect(rendered).toContain('SELECT ');
      expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('closed-union shapes select alias qualification, predicate order and original whitespace', () => {
    expect(revokedSessionHeadSql('?', 'a.company_id', 'a.user_id', 'aliased-company-first')).toBe(
      'SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=a.company_id AND r.user_id=a.user_id AND r.session_id=?',
    );
    expect(revokedSessionHeadSql('?', '?', '?', 'unaliased-company-first')).toBe(
      'SELECT 1 FROM iam_revoked_sessions WHERE company_id=? AND user_id=? AND session_id=?',
    );
    expect(revokedSessionHeadSql('c.session_id', 'c.company_id', 'c.user_id', 'aliased-session-first-multiline')).toContain('\n      AND ');
    expect(revokedSessionHeadSql('?', '?', '?', 'unaliased-session-first')).not.toBe(
      revokedSessionHeadSql('?', '?', '?', 'unaliased-company-first'),
    );
    const helper = read('adapters/read-tail-family-r36.ts');
    expect(helper).toContain("| 'aliased-session-first-multiline'");
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });
});

describe('Identity access read-tail r36 wiring', () => {
  test('one helper with one SELECT token; all seven sites route through it and keep their binds', () => {
    const helper = read('adapters/read-tail-family-r36.ts');
    expect((helper.match(/export const revokedSessionHeadSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);

    for (const h of HEADS) {
      const src = read(`adapters/${h.file}`);
      expect(src.includes(`import {revokedSessionHeadSql}`) || src.includes(`import { revokedSessionHeadSql }`), `${h.file} imports the helper`).toBe(true);
      expect(src.includes(h.call), `${h.file} routes through ${h.call}`).toBe(true);
      expect(src.includes(h.bind), `${h.file} keeps its bind`).toBe(true);
      expect(src.includes(h.sql), `${h.file} no longer inlines ${h.name}`).toBe(false);
      expect(calls(`adapters/${h.file}`, 'revokedSessionHeadSql'), h.file).toBe(2); // import + use
    }
  });

  test('locks, existing helpers and native kinds stay untouched', () => {
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/read-tail-family-r34.ts')).toBe('95e25ead2dac0a1832535f73794691036025e14748f1c17ac342c9620b3750be');
    expect(hash('adapters/read-tail-family-r35.ts')).toBe('8067ca1ffbb2a9076808a97275b1659b592eafbc2b80915e1260ffd5e2f8d442');
    expect(hash('adapters/payroll-cancellation-grant.ts')).toBe('eae4d8d3f2eacc4839676d3fcf7cd2da7edc3bb5a6a875e71b2787cf9e26e5ee');
    // The remaining aliased 'platform' probe is a different shape and stays inline.
    expect(read('adapters/platform-commercial-authority.ts')).toContain("EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id='platform'");
    expect(read('adapters/iam-probe-sql.ts')).toContain('SELECT 1 FROM iam_revoked_sessions WHERE session_id=${sessionParam}');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/account-preference-session.ts')).toContain('SELECT id FROM session WHERE id=? FOR SHARE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });

  test('post-dedupe files match their pinned byte oracles', () => {
    const pins: [string, string][] = [
      ['adapters/read-tail-family-r36.ts', 'c2d4f046bff69a0fc400d1bc07e5361f69d2fbe9a45c98c6ec8aa8b70485d9aa'],
      ['adapters/iam-store.ts', '827ec154f6723367e4974fa0e58fbf86843b779fd68166a6c8897b8a48c3c1fd'],
      ['adapters/executive-authority-source.ts', 'd6526a6b37bdcaa1b17eb55e319bf936b5431d46e8ebaa8b142adcc57baa6170'],
      ['adapters/iam-directory-refresh.ts', '9c73eb793a2bc879a6638ade55b8bd844b7f96f498545e096a8b3a693cb2f7d9'],
      ['adapters/project-worker-startup-subject.ts', 'c83fd8ea6b1a360d859695448d95bba90ffefcecc6f221615d9b45361c559c75'],
      ['adapters/queued-command-authority.ts', '6a163101d1fa28f0576b598990d8c8172a6aedc94a43f6f5f37aee12aef59d64'],
      ['adapters/queued-social-source-authority.ts', 'a5379950fb6735056405839f43758dd14235fc8da7402097cb120e780a77663e'],
      ['adapters/collaboration-push-session.ts', '49b8d99615997a27bc8999eecd699c7666d99b01fc35b922ce464f4e21e1ace8'],
    ];
    for (const [rel, sha] of pins) expect(hash(rel), rel).toBe(sha);
  });
});

describe('Identity access read-tail r36 retirement check', () => {
  test('retirement:ts-statements is 424 after wave-2 lane 10 (r42 was 438)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(454);
    expect(total).toBe(424);
  });
});
