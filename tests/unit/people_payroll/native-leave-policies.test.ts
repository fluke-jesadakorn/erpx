import { afterEach, describe, expect, it, vi } from 'vitest';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

vi.mock('server-only', () => ({}));

import { nativeActiveLeavePolicySettings } from '../../../packages/server-people_payroll/src/adapters/native-leave-policies';

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44991',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', host: 'acme.example.com', holdingId: 'h001', holdingSlug: 'acme' }],
});
const actor = {
  userId: 'u1', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-co',
  branchId: 'b1', departmentId: null, assignmentId: 'a1', modules: ['people'],
} as unknown as WorkContext;

afterEach(() => vi.unstubAllEnvs());

describe('native active leave-policy transport', () => {
  it('forwards the exact Company target and accepts the closed row shape', async () => {
    vi.stubEnv('ERPX_NATIVE_PEOPLE_PAYROLL_LEAVE_POLICIES_CONFIG', config);
    const seen: Array<{ url: URL; init: { method: string; headers: Record<string, string> } }> = [];
    const rows = await nativeActiveLeavePolicySettings(actor, async (url, init) => {
      seen.push({ url, init });
      return Response.json([{ name: 'Level 1', version: 2, status: 'active' }]);
    });
    expect(rows).toEqual([{ name: 'Level 1', version: 2, status: 'active' }]);
    expect(seen[0]!.url.pathname).toBe('/api/v1/holdings/acme/people-payroll/active-leave-policies');
    expect(seen[0]!.init.method).toBe('GET');
    expect(seen[0]!.init.headers.host).toBe('acme.example.com');
    expect(seen[0]!.init.headers['x-erpx-context-company']).toBe('acme-co');
  });

  it('fails closed on configuration, status, and malformed rows', async () => {
    await expect(nativeActiveLeavePolicySettings(actor)).rejects.toMatchObject({
      status: 503, code: 'PEOPLE_PAYROLL_LEAVE_POLICIES_UNAVAILABLE',
    });
    vi.stubEnv('ERPX_NATIVE_PEOPLE_PAYROLL_LEAVE_POLICIES_CONFIG', config);
    await expect(nativeActiveLeavePolicySettings(actor, async () => Response.json(
      { error: { code: 'FORBIDDEN' } }, { status: 403 },
    ))).rejects.toMatchObject({ status: 403, code: 'FORBIDDEN' });
    await expect(nativeActiveLeavePolicySettings(actor, async () => Response.json(
      [{ name: 'Level 1', version: '2', status: 'active' }],
    ))).rejects.toMatchObject({ status: 503 });
  });
});
