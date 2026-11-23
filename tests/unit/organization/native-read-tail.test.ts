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
const {workReportOrganizationFacts} = await import('@erpx/server-organization/public/work-report-facts');
const {activeSpendingPosition} = await import('@erpx/server-organization/adapters/spending-position');
const {pmsRolloutOrganizationFacts} = await import('@erpx/server-organization/adapters/pms-rollout-facts');
const {calendarAccessOrganizationFacts} = await import('@erpx/server-organization/adapters/calendar-access-facts');

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

  it('reads work-report nodes by typed kind and keeps the retained results shape', async () => {
    all.mockClear();
    const handle = db();
    const rows = [{id: 'node', display_name: 'Node'}] as never[];
    all.mockResolvedValueOnce(rows);
    await expect(workReportOrganizationFacts(handle).nodes('company')).resolves.toEqual({results: rows});
    expect(all).toHaveBeenCalledWith(handle, 'company', 'work_report_organization_nodes', ['company']);
  });

  it('reduces the active-spending-position fact from a typed single-row owner read', async () => {
    one.mockClear();
    const handle = db();
    one.mockResolvedValueOnce({one: 1} as never);
    await expect(activeSpendingPosition(handle, 'company', 'position')).resolves.toBe(true);
    one.mockResolvedValueOnce(null as never);
    await expect(activeSpendingPosition(handle, 'company', 'position')).resolves.toBe(false);
    expect(one).toHaveBeenCalledWith(handle, 'company', 'spending_active_position', ['position', 'company']);
  });

  it('reduces the PMS rollout active-branch fact from a typed single-row owner read', async () => {
    one.mockClear();
    const handle = db();
    const row = {'1': 1} as never;
    one.mockResolvedValueOnce(row);
    await expect(pmsRolloutOrganizationFacts(handle).organization('company')).resolves.toBe(row);
    one.mockResolvedValueOnce(null as never);
    await expect(pmsRolloutOrganizationFacts(handle).organization('company')).resolves.toBeNull();
    expect(one).toHaveBeenCalledWith(handle, 'company', 'pms_rollout_active_branch', ['company']);
  });

  it('reads calendar-access organization nodes by typed kind and keeps the row-list shape', async () => {
    all.mockClear();
    const handle = db();
    const rows = [{id: 'node', parentId: null, name: 'Node', kind: 'branch', branchId: null}] as never[];
    all.mockResolvedValueOnce(rows);
    await expect(calendarAccessOrganizationFacts(handle).organizationNodes('company')).resolves.toBe(rows);
    expect(all).toHaveBeenCalledWith(handle, 'company', 'calendar_access_organization_nodes', ['company']);
  });

});
