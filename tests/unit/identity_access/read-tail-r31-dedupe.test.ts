import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, test, vi } from 'vitest';
import { workforceActorScopeSql } from '../../../modules/identity-access/server/src/adapters/read-tail-family-r31';
import { revokedSessionProbeSql } from '../../../modules/identity-access/server/src/adapters/iam-probe-sql';
import {
  privateFileDenialAuthoritySourceLocks,
  privateFileDenialAuthorityWitness,
  setPrivateFileDenialWitnessForTest,
} from '../../../modules/identity-access/server/src/adapters/native-private-file-denial-authority';
import type { WorkContext } from '../../../modules/identity-access/server/src/public/authorization';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const srcRoot = resolve(__dirname, '../../../modules/identity-access/server/src');
const read = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const hash = (rel: string) => createHash('sha256').update(read(rel)).digest('hex');
const calls = (rel: string, name: string) => (read(rel).match(new RegExp(`\\b${name}\\b`, 'g')) ?? []).length;

// Pre-dedupe literals captured from HEAD. Rendered helper output must match exactly.
const O_HEAD = `SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE `;
const O_ADMIN_TAIL = ` AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND version=?)`;
const O_REVOKED = `SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id='platform' AND user_id=?`;
const ORACLE_WHERE = `EXISTS(SELECT 1 FROM assignments g WHERE g.id=?)`;

describe('Identity access read-tail r31 byte oracles', () => {
  test('workforce actor-scope head renders byte-identically at both call sites', () => {
    expect(workforceActorScopeSql(ORACLE_WHERE)).toBe(`${O_HEAD}${ORACLE_WHERE}`);
    expect(`${workforceActorScopeSql(ORACLE_WHERE)}${O_ADMIN_TAIL}`).toBe(
      `${O_HEAD}${ORACLE_WHERE} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND company_id=? AND version=?)`);
    expect(workforceActorScopeSql('')).toBe(O_HEAD);
  });

  test('quoted-literal company argument renders the original revoked-session probe', () => {
    expect(revokedSessionProbeSql('?', "'platform'", '?')).toBe(O_REVOKED);
    expect(revokedSessionProbeSql('?', "'platform'", '?')).not.toBe(revokedSessionProbeSql('?', '?', '?'));
  });
});

describe('Identity access read-tail r31 wiring', () => {
  test('one head definition; both call sites use it and keep tails and binds', () => {
    const helper = read('adapters/read-tail-family-r31.ts');
    expect((helper.match(/export const workforceActorScopeSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(2);
    expect(calls('adapters/admin-workspace.ts', 'workforceActorScopeSql')).toBe(2);
    expect(calls('adapters/attachment-read.ts', 'workforceActorScopeSql')).toBe(2);
    const admin = read('adapters/admin-workspace.ts');
    const attachment = read('adapters/attachment-read.ts');
    expect(admin).toContain('workforceActorScopeSql(current.where)');
    expect(admin).toContain(O_ADMIN_TAIL);
    expect(admin).toContain('.bind(actor.companyId,actor.branchId,actor.departmentId,...current.args,assignment.id,actor.companyId,assignment.version!)');
    expect(attachment).toContain('workforceActorScopeSql(currentScope.where)');
    expect(attachment).toContain('.bind(actor.companyId,actor.branchId,actor.departmentId,...currentScope.args)');
    expect(admin).not.toContain('SELECT 1 FROM(SELECT ?::text company_id');
    expect(attachment).not.toContain('SELECT 1 FROM(SELECT ?::text company_id');
  });

  test('revoked probe is imported once and called once with the quoted literal', () => {
    const src = read('adapters/authorization.ts');
    expect(calls('adapters/authorization.ts', 'revokedSessionProbeSql')).toBe(2);
    expect(src).toContain(`revokedSessionProbeSql('?', "'platform'", '?')`);
    expect(src).toContain('.bind(sessionId, userId).first()');
    expect(src).not.toContain(`prepare("SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id='platform'`);
    expect(calls('adapters/iam-probe-sql.ts', 'revokedSessionProbeSql')).toBe(1);
  });

  test('locks, session authority, native kinds and r1-r30 helpers stay untouched', () => {
    const helper = read('adapters/read-tail-family-r31.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(hash('adapters/iam-probe-sql.ts')).toBe('e0984c3774b38a3229fe848061482e46efd984f26e86152ce9973c3c49104101');
    expect(hash('adapters/read-tail-family-r30.ts')).toBe('2cde8f94073ca753169c63c8024ea217282a953d2420a1ae6f60c056a11675a0');
    expect(hash('adapters/read-tail-family-r29.ts')).toBe('2c392291ac6bfa7558a022f750fba0742acfe7014702875d8836321063650775');
    expect(read('adapters/workflow-command-authority.ts')).toContain('SELECT id FROM assignments WHERE id=? FOR SHARE');
    expect(read('adapters/product-sso-store.ts')).toContain('SELECT * FROM sso_browser_transactions WHERE id=? FOR UPDATE');
    expect(readdirSync(srcRoot, { recursive: true }).filter((f) => String(f).endsWith('.rs')).length).toBeGreaterThan(0);
  });
});

describe('Identity access private-file denial native witness', () => {
  const ENV = 'ERPX_NATIVE_PRIVATE_FILE_DENIAL_CONFIG';
  const actor = (overrides: Record<string, unknown> = {}) =>
    ({
      companyId: 'company-1',
      userId: 'user-1',
      assignmentId: 'assignment-1',
      role: 'it_admin',
      holdingId: 'holding-1',
      holdingSlug: 'acme',
      authority: { sessionId: 'session-1' },
      ...overrides,
    }) as unknown as WorkContext;
  const database = { prepare: (sql: string) => ({ sql }) } as unknown as SqlDatabase;
  const config = (holdings: unknown[] = [{
    origin: 'https://acme.example.test', host: 'acme.example.test',
    holdingId: 'holding-1', holdingSlug: 'acme',
  }]) => JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44991', timeoutMs: 1000, holdings });

  afterEach(() => {
    setPrivateFileDenialWitnessForTest(undefined);
    vi.unstubAllEnvs();
  });

  test('the replaced TS image query is gone and the native bridge carries no SQL', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/private-file-denial-authority.ts'))).toBe(false);
    const client = read('adapters/native-private-file-denial-authority.ts');
    expect(client).not.toMatch(/SELECT |WITH RECURSIVE|jsonb_build_object/i);
    const entry = read('public/private-file-denial-authority.ts');
    expect(entry).toContain('native-private-file-denial-authority');
    expect(entry).not.toContain('adapters/private-file-denial-authority');
  });

  test('fails closed when the native owner is missing or misconfigured', async () => {
    vi.stubEnv(ENV, undefined);
    await expect(privateFileDenialAuthorityWitness(database, actor())).rejects.toMatchObject({
      code: 'PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_REQUIRED',
    });
    vi.stubEnv(ENV, 'not-json');
    await expect(privateFileDenialAuthorityWitness(database, actor())).rejects.toMatchObject({
      code: 'PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID',
    });
    vi.stubEnv(ENV, config([{ origin: 'http://acme.example.test', host: 'acme.example.test', holdingId: 'holding-1', holdingSlug: 'acme' }]));
    await expect(privateFileDenialAuthorityWitness(database, actor())).rejects.toMatchObject({
      code: 'PRIVATE_FILE_DENIAL_AUTHORITY_CONFIGURATION_INVALID',
    });
  });

  test('routes by exact holding and only accepts a verified 64-hex witness', async () => {
    vi.stubEnv(ENV, config());
    const seen: unknown[] = [];
    setPrivateFileDenialWitnessForTest(async (witnessActor, holding) => {
      seen.push({ witnessActor, holding });
      return 'a'.repeat(64);
    });
    expect(await privateFileDenialAuthorityWitness(database, actor())).toBe('a'.repeat(64));
    expect(seen).toEqual([{
      witnessActor: { companyId: 'company-1', userId: 'user-1', assignmentId: 'assignment-1', role: 'it_admin', sessionId: 'session-1' },
      holding: { origin: 'https://acme.example.test', host: 'acme.example.test', holdingId: 'holding-1', holdingSlug: 'acme' },
    }]);
    await expect(privateFileDenialAuthorityWitness(database, actor({ holdingSlug: 'other' }))).rejects.toMatchObject({
      code: 'PRIVATE_FILE_DENIAL_AUTHORITY_TARGET_UNAVAILABLE',
    });
    setPrivateFileDenialWitnessForTest(async () => 'A'.repeat(64));
    await expect(privateFileDenialAuthorityWitness(database, actor())).rejects.toMatchObject({
      code: 'PRIVATE_FILE_DENIAL_RESPONSE_INVALID',
    });
  });

  test('preserves the owner source lease without issuing a native read in TypeScript', () => {
    const locks = privateFileDenialAuthoritySourceLocks(database) as unknown as { sql: string }[];
    expect(locks).toHaveLength(1);
    expect(locks[0]!.sql).toBe('LOCK TABLE iam_permissions IN SHARE MODE');
  });
});

describe('Identity access read-tail r31 retirement check', () => {
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
