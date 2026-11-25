import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const h = vi.hoisted(() => ({
  email: vi.fn(),
  status: vi.fn(),
  invitation: vi.fn(),
}));
const clientPath = '../../../packages/server-identity_access/src/adapters/native-lane-10/client';
vi.mock('server-only', () => ({}));
vi.mock('../../../packages/server-runtime/src/public/runtime', () => ({
  runtimeEnv: async () => ({ BETTER_AUTH_SECRET: 'lane10-test-secret', CONTROL_DB: {} }),
  holdingResources: async () => ({ write: {} }),
}));

const srcRoot = resolve(__dirname, '../../../packages/server-identity_access/src');
const source = (rel: string) => readFileSync(resolve(srcRoot, rel), 'utf8');
const devConfig = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'platform' },
  ],
};
const fixture = {
  id: 'inv_1',
  holding_id: 'h001',
  company_id: 'c1',
  person_id: 'p1',
  email: 'person@example.test',
  name: 'Person',
  role: '{"existingUserId":"u1"}',
  scope_kind: 'holding',
  branch_id: null,
  department_id: null,
  created_by: 'u2',
  assignment_id: 'a1',
  expires_at: 1_800_000_000_000,
  status: 'pending',
};

describe('native lane 10 TypeScript boundary', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.doMock(clientPath, () => ({
      nativeEmailExists: h.email,
      nativeInvitationStatus: h.status,
      nativeInvitationByToken: h.invitation,
    }));
  });
  afterEach(() => {
    vi.doUnmock(clientPath);
    vi.resetModules();
    vi.unstubAllEnvs();
    vi.clearAllMocks();
  });

  it('deletes every owner SELECT and keeps no database fallback in account-access.ts', () => {
    const adapter = source('adapters/account-access.ts');
    expect(adapter).not.toMatch(/SELECT\s/i);
    expect(adapter).not.toMatch(/CONTROL_DB\.prepare/);
    expect(adapter).toContain("from './native-lane-10/client'");
    expect(source('adapters/native-lane-10/client.ts')).not.toMatch(/\.prepare\(|SELECT\s/i);
  });

  it('cuts inspectInvitation over to the native reader using the hashed token', async () => {
    h.invitation.mockResolvedValueOnce(fixture);
    const { inspectInvitation } = await import('../../../packages/server-identity_access/src/adapters/account-access');
    const token = 'lane10-token-32-characters-minimum';
    const result = await inspectInvitation(token);
    expect(result).toEqual({ name: 'Person', email: 'person@example.test', existingAccount: true });
    expect(h.invitation).toHaveBeenCalledOnce();
    const [tokenHash, now] = h.invitation.mock.calls[0]!;
    expect(tokenHash).toBe(createHash('sha256').update(token).digest('hex'));
    expect(now).toBeGreaterThan(1_000_000_000_000);
  });

  it('propagates fail-closed native errors without a local SQL retry', async () => {
    const { ApiError } = await import('@erpx/server-runtime/public/http');
    h.invitation.mockRejectedValueOnce(new ApiError(503, 'ACCOUNT_ACCESS_READ_UNAVAILABLE', 'unavailable'));
    const { inspectInvitation } = await import('../../../packages/server-identity_access/src/adapters/account-access');
    await expect(inspectInvitation('lane10-token-32-characters-minimum')).rejects.toMatchObject({
      status: 503,
      code: 'ACCOUNT_ACCESS_READ_UNAVAILABLE',
    });
  });
});

describe('native lane 10 multi-holding client', () => {
  let client: typeof import('../../../packages/server-identity_access/src/adapters/native-lane-10/client');
  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    vi.stubEnv('ERPX_NATIVE_LANE_10_CONFIG', JSON.stringify(devConfig));
    client = await import(clientPath);
  });
  afterEach(() => {
    client.setAccountAccessReadForTest(undefined);
    vi.unstubAllEnvs();
    vi.restoreAllMocks();
    vi.resetModules();
  });

  it('selects the exact registered holding by holdingId + holdingSlug', async () => {
    const seen: Array<{ kind: string; values: Record<string, unknown>; company: string; host: string; slug: string }> = [];
    client.setAccountAccessReadForTest(async (kind, values, company, target) => {
      seen.push({ kind, values, company, host: target.host, slug: target.holdingSlug });
      if (kind === 'email_exists') return true;
      if (kind === 'invitation_status') return 'pending';
      return fixture;
    });
    expect(await client.nativeEmailExists('person@example.test', { holdingId: 'h001', holdingSlug: 'acme', companyId: 'c1' })).toBe(true);
    expect(await client.nativeInvitationStatus({ holdingId: 'h002', holdingSlug: 'northstar', companyId: 'c2' }, 'p1', 'inv_1')).toBe('pending');
    expect(await client.nativeInvitationByToken('a'.repeat(64), 1_700_000_000_000)).toEqual(fixture);
    expect(seen).toEqual([
      { kind: 'email_exists', values: { email: 'person@example.test' }, company: 'c1', host: 'acme.jesadakorn.com', slug: 'acme' },
      { kind: 'invitation_status', values: { holding_id: 'h002', person_id: 'p1', invitation_id: 'inv_1' }, company: 'c2', host: 'northstar.jesadakorn.com', slug: 'northstar' },
      { kind: 'invitation_by_token', values: { token_hash: 'a'.repeat(64), now: 1_700_000_000_000 }, company: 'platform', host: 'erpx-internal.jesadakorn.com', slug: 'erpx' },
    ]);
    expect(createHash('sha256').update(JSON.stringify({ kind: 'invitation_status', value: 'pending' })).digest('hex')).toBe(
      '45f6739eba125a1ad4c1b6d010ae3ffafd10d8bd5976e45fb032cdc24e948aa7',
    );
  });

  it('fails closed on unknown or mismatched targets with no first-holding default', async () => {
    for (const scope of [
      { holdingId: 'h999', holdingSlug: 'acme', companyId: 'c1' },
      { holdingId: 'h001', holdingSlug: 'northstar', companyId: 'c1' },
      { holdingId: 'h002', holdingSlug: 'acme', companyId: 'c1' },
    ])
      await expect(client.nativeInvitationStatus(scope, 'p1', 'inv_1')).rejects.toMatchObject({
        status: 503,
        code: 'ACCOUNT_ACCESS_READ_TARGET_UNAVAILABLE',
      });
    vi.stubEnv('ERPX_NATIVE_LANE_10_CONFIG', JSON.stringify({ ...devConfig, holdings: [] }));
    await expect(client.nativeEmailExists('person@example.test')).rejects.toMatchObject({
      status: 503,
      code: 'ACCOUNT_ACCESS_READ_CONFIGURATION_INVALID',
    });
  });

  it('fails closed on malformed shapes and transport failures', async () => {
    client.setAccountAccessReadForTest(async () => ({ kind: 'email_exists', value: { unexpected: true } }));
    await expect(client.nativeEmailExists('person@example.test')).rejects.toMatchObject({ status: 503, code: 'ACCOUNT_ACCESS_READ_RESPONSE_INVALID' });
    client.setAccountAccessReadForTest(undefined);
    const fetchSpy = vi.spyOn(globalThis, 'fetch').mockRejectedValueOnce(new Error('network'));
    await expect(client.nativeEmailExists('person@example.test')).rejects.toMatchObject({
      status: 503,
      code: 'ACCOUNT_ACCESS_READ_UNAVAILABLE',
    });
    expect(fetchSpy).toHaveBeenCalledOnce();
  });

  it('fails closed without configuration and rejects caller-supplied SQL', async () => {
    vi.stubEnv('ERPX_NATIVE_LANE_10_CONFIG', undefined);
    await expect(client.nativeEmailExists('person@example.test')).rejects.toMatchObject({
      status: 503,
      code: 'ACCOUNT_ACCESS_READ_CONFIGURATION_REQUIRED',
    });
    client.setAccountAccessReadForTest(async (kind) => ({ kind, value: true }));
    await expect(client.nativeEmailExists('DROP TABLE account_invitations')).rejects.toMatchObject({
      status: 422,
      code: 'ACCOUNT_ACCESS_READ_REQUEST_INVALID',
    });
  });
});
