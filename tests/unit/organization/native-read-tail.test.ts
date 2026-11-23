import {describe,expect,it,vi} from 'vitest';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

// The retired owner-local readers build no SQL of their own. Mock the native
// transport and assert the exact typed kind and positional values they send.
const one = vi.hoisted(() => vi.fn(async (_db: unknown, _companyId: string, _kind: string, _values: unknown[]) => null as unknown));
const all = vi.hoisted(() => vi.fn(async (_db: unknown, _companyId: string, _kind: string, _values: unknown[]) => [] as unknown[]));
vi.mock('../../../packages/server-organization/src/adapters/native-organization-owner-write', () => ({
  nativeOrgOne: one,
  nativeOrgAll: all,
}));
const {companyAcceptanceResultProjection} = await import('@erpx/server-organization/adapters/company-acceptance-result-projection');
const {activeDepartmentFact} = await import('@erpx/server-organization/adapters/profile-facts');
const {publicMarketingReceivingBranch} = await import('@erpx/server-organization/adapters/public-marketing-receiving-branch');

function db(): SqlDatabase {
  return {prepare: vi.fn(() => {throw new Error('owner-local reader must not build local SQL');})} as unknown as SqlDatabase;
}

describe('Organization owner-local read tail', () => {
  it('reads the acceptance result projection by typed kind with only positional values', async () => {
    all.mockClear();
    const rows = [{id: 'company-acceptance-result-key-a', company_id: 'company'}] as never[];
    all.mockResolvedValueOnce(rows);
    const handle = db();
    await expect(companyAcceptanceResultProjection(handle, 'company', 'key', 1, 2)).resolves.toBe(rows);
    expect(all).toHaveBeenCalledWith(handle, 'company', 'company_acceptance_result_projection', ['key', 1, 2, 'company']);
  });

  it('reduces the active-department fact from a typed single-row owner read', async () => {
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({one: 1} as never);
    await expect(activeDepartmentFact(handle, 'company', 'department')).resolves.toBe(true);
    one.mockResolvedValueOnce(null as never);
    await expect(activeDepartmentFact(handle, 'company', 'department')).resolves.toBe(false);
    expect(one).toHaveBeenCalledWith(handle, 'company', 'profile_active_department', ['department', 'company']);
  });

  it('resolves the public-form receiving branch by typed kind with only the Company value', async () => {
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({id: 'branch'} as never);
    await expect(publicMarketingReceivingBranch(handle, 'company')).resolves.toEqual({id: 'branch'});
    expect(one).toHaveBeenCalledWith(handle, 'company', 'public_marketing_receiving_branch', ['company']);
  });

});
