import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
const list = vi.fn();
const status = vi.fn();
const mapped = vi.fn();
vi.mock('server-only', () => ({}));
vi.mock('@erpx/feature-work_projection/lib/destination-links', () => ({
  canonicalDestinationLinks: (value: unknown) => value,
}));
vi.mock('@erpx/server-files/public/attachments/feature', () => ({
  attachmentsV2ReadOnly: () => false,
}));
vi.mock('@erpx/server-files/public/native-attachments', () => ({
  nativeAttachmentReadAvailable: () => process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG !== undefined,
  nativeAttachmentReadConfiguration: (raw: string) => JSON.parse(raw),
  createNativeAttachmentReadClient: () => ({ list, status }),
  nativeAttachmentReadFailure: (error: unknown) => mapped(error),
}));
vi.mock('@erpx/server-runtime/public/http', () => ({
  ApiError: class ApiError extends Error {
    constructor(
      readonly status: number,
      readonly code: string,
      message: string,
    ) {
      super(message);
    }
  },
}));

import {
  nativeAttachmentReadConfigured,
  nativeAttachmentSets,
  nativeAttachmentStatus,
} from '../../../packages/workplace/src/lib/native-attachments';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

const actor = {
  userId: 'actor',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: 'branch',
  departmentId: null,
  assignmentId: 'assignment-actor',
  role: 'owner',
  assignmentScope: 'branch',
  scope: { kind: 'branch', branchSlug: 'north' },
  warehouseAuthority: { revision: 7, assignmentVersion: 3 },
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['records'],
  iam: { permissions: ['records.read'] },
} as unknown as WorkContext;
const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44993',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
});
const request = () =>
  new Request('https://acme.example.com/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1', {
    headers: { host: 'acme.example.com', cookie: 'session' },
  });

beforeEach(() => {
  list.mockReset();
  status.mockReset();
  mapped.mockReset();
});
afterEach(() => {
  delete process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG;
});

describe('native Files attachment Next cutover', () => {
  it('retains the TypeScript path only when no native configuration exists', async () => {
    expect(nativeAttachmentReadConfigured()).toBe(false);
    await expect(nativeAttachmentSets(request(), actor, new URLSearchParams())).resolves.toBeUndefined();
    await expect(nativeAttachmentStatus(request(), actor, 'link-1')).resolves.toBeUndefined();
    expect(list).not.toHaveBeenCalled();
    expect(status).not.toHaveBeenCalled();
  });

  it('forwards the exact selected target and branch scope through the native reader', async () => {
    process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG = config;
    list.mockResolvedValue([{ id: 'set-1' }]);
    status.mockResolvedValue({ linkId: 'link-1' });
    const sets = await nativeAttachmentSets(
      request(),
      actor,
      new URLSearchParams({ targetType: 'asset', targetId: 'asset-1', purposeKey: 'manual' }),
    );
    expect(sets?.status).toBe(200);
    expect(await sets?.json()).toEqual({ data: [{ id: 'set-1' }] });
    expect(list).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({
        userId: 'actor',
        holdingSlug: 'acme',
        scope: { kind: 'branch', branchSlug: 'north' },
        authority: { revision: 7, assignmentVersion: 3 },
      }),
      expect.objectContaining({
        holdingSlug: 'acme',
        targetType: 'asset',
        targetId: 'asset-1',
        purposeKey: 'manual',
        readOnly: false,
      }),
    );
    const result = await nativeAttachmentStatus(request(), actor, 'link-1');
    expect(result?.status).toBe(200);
    expect(status).toHaveBeenCalledWith(expect.anything(), expect.anything(), 'acme', 'link-1');
  });

  it('surfaces a configured native failure without falling back', async () => {
    process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG = config;
    mapped.mockReturnValue({
      status: 409,
      code: 'ATTACHMENT_READ_CHANGED',
      message: 'The file metadata changed while it was being read.',
    });
    list.mockRejectedValue(new Error('changed'));
    await expect(
      nativeAttachmentSets(request(), actor, new URLSearchParams({ targetType: 'asset', targetId: 'asset-1' })),
    ).rejects.toMatchObject({
      status: 409,
      code: 'ATTACHMENT_READ_CHANGED',
    });
    expect(list).toHaveBeenCalledTimes(1);
    expect(mapped).toHaveBeenCalledTimes(1);
    expect(new ApiError(0, '', '')).toBeInstanceOf(Error);
  });

  it('maps an unconverted configured failure to the retained unavailable surface', async () => {
    process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG = config;
    mapped.mockReturnValue(undefined);
    list.mockRejectedValue(new Error('socket'));
    await expect(
      nativeAttachmentSets(request(), actor, new URLSearchParams({ targetType: 'asset', targetId: 'asset-1' })),
    ).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_ATTACHMENT_READER_UNAVAILABLE',
    });
  });
});
