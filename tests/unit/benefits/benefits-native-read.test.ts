import { beforeEach, describe, expect, it, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const source = (path: string) => readFileSync(
  resolve(__dirname, '../../../modules/benefits/server/src', path),
  'utf8',
);

const { nativeWorkspace, nativeDetail } = vi.hoisted(() => ({
  nativeWorkspace: vi.fn(),
  nativeDetail: vi.fn(),
}));
vi.mock('@erpx/benefits-ui/public/native-benefits-workspace-page', () => ({
  nativeBenefitsWorkspace: nativeWorkspace,
  nativeBenefitDetail: nativeDetail,
}));

import { ApiError } from '@erpx/platform-runtime-server/public/http';
import {
  benefitsWorkspace,
  benefitDetail,
  benefitWork,
} from '@erpx/benefits-server/adapters/benefits-native-read';
import { benefitsWorkspace as portWorkspace, benefitDetail as portDetail, benefitWork as portWork } from '@erpx/benefits-server/public/benefits-native-read';

const actor = { userId: 'user' } as never;
const election = (id: string, personId: string, status: string, updatedAt = 20) => ({
  id, company_id: 'company', person_id: personId, engagement_id: 'engagement',
  plan_id: 'plan', plan_name: 'Family plan', employee_name: 'Employee',
  plan_version: 1, window_id: 'window', window_version: 1, coverage_type: 'family',
  currency: 'THB', employee_deduction_minor: 100, employer_cost_minor: 200,
  effective_from: '2027-01-01', effective_to: null, status, version: 1,
  created_at: 10, updated_at: updatedAt,
});
const data = (overrides: Record<string, unknown> = {}) => ({
  actorId: 'user', canWrite: true, canReview: false, canManageDocuments: false,
  workContext: 'a'.repeat(64), plans: [], windows: [], dependents: [], elections: [],
  ...overrides,
});

beforeEach(() => {
  nativeWorkspace.mockReset();
  nativeDetail.mockReset();
});

describe('Benefits native read delegation', () => {
  it('exposes the native module through the public port', () => {
    expect(portWorkspace).toBe(benefitsWorkspace);
    expect(portDetail).toBe(benefitDetail);
    expect(portWork).toBe(benefitWork);
  });

  it('delegates the workspace and detail reads with the original arguments', async () => {
    const workspace = data({ plans: [{ id: 'plan' }] });
    const detail = { election: { id: 'election-1' }, history: [], payroll: null };
    nativeWorkspace.mockResolvedValue(workspace);
    nativeDetail.mockResolvedValue(detail);
    expect(await benefitsWorkspace(actor, true)).toBe(workspace);
    expect(nativeWorkspace).toHaveBeenCalledWith(actor, true);
    expect(await benefitDetail(actor, 'election-1')).toBe(detail);
    expect(nativeDetail).toHaveBeenCalledWith(actor, 'election-1');
  });

  it('does not turn a 403 or 404 workspace or detail error into an empty success', async () => {
    const error = new ApiError(403, 'PACKAGE_REQUIRED', 'History required.');
    nativeWorkspace.mockRejectedValue(error);
    nativeDetail.mockRejectedValue(error);
    await expect(benefitsWorkspace(actor)).rejects.toBe(error);
    await expect(benefitDetail(actor, 'election-1')).rejects.toBe(error);
  });
});

describe('Benefits native work projection', () => {
  it('keeps only active elections and derives the actor Person from dependents', async () => {
    nativeWorkspace.mockResolvedValue(data({
      elections: [
        election('e1', 'person-owner', 'elected', 50),
        election('e2', 'person-owner', 'submitted', 40),
        election('e3', 'person-owner', 'acknowledged', 30),
        election('e4', 'person-owner', 'released', 20),
        election('e5', 'person-other', 'cancelled', 10),
      ],
      dependents: [{ id: 'dependent', person_id: 'person-owner' }],
    }));
    const items = await benefitWork(actor);
    expect(items.map(item => item.id)).toEqual(['e1', 'e2', 'e3']);
    expect(items.map(item => item.relationships)).toEqual([
      ['requested'],
      ['requested', 'approval'],
      ['requested', 'approval'],
    ]);
    expect(items.map(item => item.queue)).toEqual(['todo', 'waiting', 'waiting']);
  });

  it('derives the Person from elections for a personal scope without dependents', async () => {
    nativeWorkspace.mockResolvedValue(data({
      elections: [election('e1', 'person-owner', 'elected')],
      canReview: false,
    }));
    expect((await benefitWork(actor))[0].relationships).toEqual(['requested']);
  });

  it('treats reviewer elections without a derivable Person as team work', async () => {
    nativeWorkspace.mockResolvedValue(data({
      elections: [election('e1', 'person-owner', 'elected')],
      canReview: true,
    }));
    const items = await benefitWork(actor);
    expect(items[0].relationships).toEqual(['team']);
    expect(items[0].canAct).toBe(false);
  });

  it('redacts actionable and approval relationships when existing work is not purchased', async () => {
    const purchase = { newWork: true, settleExisting: false, history: true, revision: 'r1' };
    nativeWorkspace.mockResolvedValue(data({
      elections: [election('e1', 'person-owner', 'submitted')],
      dependents: [{ id: 'dependent', person_id: 'person-owner' }],
      canReview: true,
      purchase,
    }));
    expect(await benefitWork(actor)).toMatchObject([{
      id: 'e1', canAct: false, queue: 'waiting', relationships: ['requested'],
    }]);
  });

  it('keeps actionable work when existing work is purchased or package controls are absent', async () => {
    for (const purchase of [
      undefined,
      { newWork: true, settleExisting: true, history: true, revision: 'r1' },
    ]) {
      nativeWorkspace.mockResolvedValue(data({
        elections: [election('e1', 'person-owner', 'submitted')],
        dependents: [{ id: 'dependent', person_id: 'person-owner' }],
        canReview: true,
        ...(purchase ? { purchase } : {}),
      }));
      expect(await benefitWork(actor)).toMatchObject([{
        id: 'e1', canAct: true, queue: 'todo', relationships: ['requested', 'approval'],
      }]);
    }
  });

  it('returns an empty queue for 403 and 404 and propagates other failures', async () => {
    for (const status of [403, 404]) {
      nativeWorkspace.mockRejectedValue(new ApiError(status, 'NOT_AVAILABLE', 'denied'));
      expect(await benefitWork(actor)).toEqual([]);
    }
    const error = new ApiError(503, 'NATIVE_BENEFITS_UNAVAILABLE', 'down');
    nativeWorkspace.mockRejectedValue(error);
    await expect(benefitWork(actor)).rejects.toBe(error);
  });
});

describe('Benefits native read port carries no TypeScript SQL', () => {
  it('contains no SQL, database handle or fallback in the adapter or public barrel', () => {
    for (const path of ['adapters/benefits-native-read.ts', 'public/benefits-native-read.ts']) {
      const text = source(path);
      expect(text, path).not.toMatch(/\bSELECT\s|\bINSERT\s+INTO\b|\bDELETE\s+FROM\b|\bUPDATE\s+\w+\s+SET\b/i);
      expect(text, path).not.toMatch(/\bdatabase\b|\bprepare\s*\(/);
      expect(text, path).not.toMatch(/catch\s*\(\s*\(\)\s*=>/);
    }
  });

  it('publishes exactly the workspace, detail and work native reads', () => {
    const barrel = source('public/benefits-native-read.ts');
    for (const name of ['benefitsWorkspace', 'benefitDetail', 'benefitWork']) expect(barrel).toContain(name);
    expect(barrel.match(/export\s*\{/g) ?? []).toHaveLength(1);
  });
});
