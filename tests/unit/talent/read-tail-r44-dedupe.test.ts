import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { nativeTalentReadFirst } from '../../../packages/server-talent/src/adapters/native-talent-read';

const adapters = resolve(__dirname, '../../../packages/server-talent/src/adapters');
const src = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// The exact owner contract now lives in the native registry. Parsing the Rust
// literal keeps the oracle on the shipped bytes rather than a second copy.
const REGISTRY = src('native_talent_read_sql.rs');
const SQL_LITERAL = REGISTRY.match(
  /const SQL_RD_CANDIDATE_PRIVACY_WORKSPACE: &str =\n    ("[\s\S]*?");\nconst READ_CANDIDATE_PRIVACY_WORKSPACE/,
)![1];
const SQL = JSON.parse(SQL_LITERAL) as string;

// Closed pre-r44 renderer output: the former TS literal interpolated the
// Company scope CTE and the same four fixed permission witnesses. The nested
// EXISTS fragments always came from the shared IAM authority renderer.
const VIEWER = '(SELECT ?::text company_id,NULL::text branch_id,NULL::text department_id)';
const witness = (permission: string) =>
  `EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=privacy_viewer.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'${permission}')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=privacy_viewer.branch_id OR g.scope_kind='department' AND g.scope_node_id=privacy_viewer.department_id))`;
const GOLDEN = `WITH privacy_viewer AS ${VIEWER}
 SELECT c.id,c.status,c.version,c.retention_until,c.privacy_hold,c.privacy_requested_by,${witness('legal.hold')} can_hold,${witness('legal.hold.release')} can_release,${witness('legal.destroy')} can_erase,${witness('people.manage')} can_withdraw
 FROM talent_candidates c CROSS JOIN privacy_viewer WHERE c.id=? AND c.company_id=privacy_viewer.company_id`;
const LENGTH = 2610;
const DIGEST = '62d9d94dca19076a7a8980ba3472c340377f14c3ea480ff68dd8210f784be6d4';
const KIND = 'talent_rd_candidate_privacy_workspace';
const BINDS = 14;

const config = () =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44993',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
  });

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Talent read-tail r44 candidate privacy workspace native read', () => {
  test('the registered plan is byte-identical to the retired TypeScript literal', () => {
    expect(SQL).toBe(GOLDEN);
    expect(SQL.length).toBe(LENGTH);
    expect(sha(SQL)).toBe(DIGEST);
    expect(SQL.match(/\?/g) ?? []).toHaveLength(BINDS);
    readOnly(SQL);
    // The registry stores it once with the exact positional contract.
    expect(REGISTRY).toContain(`pub const RD_CANDIDATE_PRIVACY_WORKSPACE: &str = "${KIND}"`);
    expect(REGISTRY).toContain('sql: SQL_RD_CANDIDATE_PRIVACY_WORKSPACE,\n    bind_count: 14,');
    expect(REGISTRY.match(new RegExp(`"${KIND}"`, 'g')) ?? []).toHaveLength(1);
  });

  test('the TypeScript workspace read carries no SQL and sends only kind and values', () => {
    const callsite = src('candidate-privacy-workspace.ts');
    expect(callsite.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(callsite).not.toContain('privacy_viewer AS');
    expect(callsite).not.toContain('assignmentReadSql');
    expect(callsite).not.toContain('talentCompanyScopeCte');
    expect(callsite.match(/nativeTalentReadFirst</g) ?? []).toHaveLength(1);
    // Company scope, four fixed assignment triplets, then the candidate id.
    expect(callsite).toContain(
      `'${KIND}',[actor.companyId,...permissions.flatMap(()=>[actor.assignmentId,actor.userId,actor.role]),id],database`,
    );
    expect(callsite).toContain('nativeTalentReadAll<');
  });

  test('the transport plans the exact owner statement with fourteen positional values', async () => {
    const previous = process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
    process.env.ERPX_NATIVE_TALENT_READ_CONFIG = config();
    const calls: { kind: string; values: unknown[] }[] = [];
    const prepared: string[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn(async (_url: unknown, init: { body?: string }) => {
        const body = JSON.parse(String(init.body)) as { kind: string; values: unknown[] };
        calls.push(body);
        return new Response(JSON.stringify({ data: { statements: [{ sql: SQL, values: body.values }] } }), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        });
      }),
    );
    try {
      const values = [
        'company',
        'assignment', 'user', 'role',
        'assignment', 'user', 'role',
        'assignment', 'user', 'role',
        'assignment', 'user', 'role',
        'candidate',
      ];
      const db = {
        prepare(sql: string) {
          prepared.push(sql);
          return {
            bind(...bound: unknown[]) {
              expect(bound).toEqual(values);
              return { first: async () => ({ id: 'candidate' }) };
            },
          };
        },
      } as never;
      await expect(nativeTalentReadFirst(KIND, values, db)).resolves.toEqual({ id: 'candidate' });
      expect(calls).toEqual([{ kind: KIND, values }]);
      expect(prepared).toEqual([SQL]);
    } finally {
      vi.unstubAllGlobals();
      if (previous === undefined) delete process.env.ERPX_NATIVE_TALENT_READ_CONFIG;
      else process.env.ERPX_NATIVE_TALENT_READ_CONFIG = previous;
    }
  });

  test('lowers the talent owner SELECT KPI from BEFORE=10 to 9', () => {
    const files: string[] = [];
    const walk = (d: string) => {
      for (const e of readdirSync(d, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(d, e.name);
        if (e.isDirectory()) walk(p);
        else if (e.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(resolve(__dirname, '../../../packages/server-talent/src'));
    const contents = files.map((f) => readFileSync(f, 'utf8'));
    const matchCount = (re: RegExp) => contents.reduce((n, text) => n + (text.match(re) ?? []).length, 0);
    const splitCount = contents.reduce((n, text) => n + text.split('SELECT ').length - 1, 0);
    for (const count of [matchCount(/SELECT/g), matchCount(/\bSELECT\b/g), splitCount]) {
      expect(count).toBe(9);
      expect(count).toBeLessThan(10);
    }
  });
});
