import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect, vi } from 'vitest';
vi.mock('server-only', () => ({}));
const { native } = vi.hoisted(() => ({ native: vi.fn() }));
vi.mock('@erpx/server-identity_access/public/native-session-list', () => ({
  nativeSessionList: native,
}));
import { nativeIamSessionProbe } from '../../../packages/server-identity_access/src/adapters/native-iam-session-probe';

const row = {
  id: 'session-1',
  createdAt: '2026-01-01T00:00:00.000Z',
  expiresAt: '2026-01-08T00:00:00.000Z',
  userAgent: 'ERPX/1.0',
};
const incoming = () => new Headers({ host: 'acme.jesadakorn.com' });
const codeOf = async (run: () => Promise<unknown>) => {
  try {
    await run();
    return undefined;
  } catch (error) {
    return (error as { code?: string }).code;
  }
};

describe('account IAM catch-all session probe native cutover', () => {
  it('delegates the unbound Account actor to the native session-list read', async () => {
    native.mockResolvedValue([row]);
    await expect(nativeIamSessionProbe({ userId: 'user-1' }, incoming())).resolves.toEqual([row]);
    expect(native).toHaveBeenCalledWith(
      { userId: 'user-1', holdingId: null },
      expect.any(Headers),
    );
    expect(native.mock.calls[0][1].get('host')).toBe('acme.jesadakorn.com');
  });

  it('passes a Company binding through unchanged when the caller has one', async () => {
    native.mockResolvedValue([]);
    await expect(
      nativeIamSessionProbe({ userId: 'user-1', companyId: 'company-1' }, incoming()),
    ).resolves.toEqual([]);
    expect(native).toHaveBeenCalledWith(
      { userId: 'user-1', holdingId: null, companyId: 'company-1' },
      expect.any(Headers),
    );
  });

  it('fails closed on an unbounded actor before any native IO', async () => {
    native.mockReset();
    expect(
      await codeOf(() => nativeIamSessionProbe(undefined, incoming())),
    ).toBe('NATIVE_IAM_SESSION_PROBE_REQUEST_INVALID');
    expect(
      await codeOf(() => nativeIamSessionProbe({ userId: 'not a user' }, incoming())),
    ).toBe('NATIVE_IAM_SESSION_PROBE_REQUEST_INVALID');
    expect(
      await codeOf(() => nativeIamSessionProbe({ userId: 'user-1', companyId: 'bad id' }, incoming())),
    ).toBe('NATIVE_IAM_SESSION_PROBE_REQUEST_INVALID');
    expect(native).not.toHaveBeenCalled();
  });

  it('propagates a native failure without a TypeScript SQL fallback', async () => {
    native.mockReset();
    const failure = Object.assign(new Error('native unavailable'), {
      code: 'NATIVE_IAM_SESSION_LIST_UNAVAILABLE',
      status: 503,
    });
    native.mockRejectedValue(failure);
    await expect(nativeIamSessionProbe({ userId: 'user-1' }, incoming())).rejects.toBe(failure);
    expect(native).toHaveBeenCalledOnce();
  });
});

describe('owner source retirement', () => {
  const src = (rel: string) =>
    readFileSync(resolve(__dirname, '../../../packages/server-identity_access/src', rel), 'utf8');

  it('routes the account IAM catch-all sessions probe through the native helper', () => {
    const api = src('adapters/iam-api.ts');
    expect(api).toContain('nativeIamSessionProbe');
    expect(api).not.toMatch(/\bsessionByUserSql\(/);
  });

  it('keeps no TypeScript session-list SELECT in the IAM adapter family', () => {
    const probe = src('adapters/native-iam-session-probe.ts');
    expect(probe).not.toMatch(/SELECT\s+id[, ]+"createdAt"/i);
    expect(probe).toContain('nativeSessionList');
  });
});
