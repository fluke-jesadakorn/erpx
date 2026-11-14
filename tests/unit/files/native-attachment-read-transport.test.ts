import { describe, it, expect, vi, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));

import {
  createNativeAttachmentCommandClient,
  createNativePortalAttachmentCommandClient,
  createNativeAttachmentReadClient,
  nativeAttachmentCommandAvailable,
  nativeAttachmentCommandConfiguration,
  nativeAttachmentCommandFailure,
  nativeAttachmentReadAvailable,
  nativeAttachmentReadConfiguration,
  nativeAttachmentReadFailure,
  NativeAttachmentReadError,
} from '../../../packages/server-files/src/public/native-attachments';
import type { NativeAttachmentPageContext } from '../../../packages/server-files/src/public/native-attachments';

const config = (overrides: Record<string, unknown> = {}) =>
  JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44993',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
    ...overrides,
  });
const parsed = (overrides: Record<string, unknown> = {}) =>
  nativeAttachmentReadConfiguration(config(overrides));
const incoming = (overrides: Record<string, string> = {}) =>
  new Headers({
    host: 'acme.example.com',
    cookie: '__Host-erpx-product=session',
    ...overrides,
  });
const actor: NativeAttachmentPageContext = {
  userId: 'actor',
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  assignmentId: 'assignment-actor',
  role: 'owner',
  assignmentScope: 'holding',
  scope: { kind: 'holding' },
  authority: { revision: 7, assignmentVersion: 3 },
  modules: ['records'],
  iam: { permissions: ['records.read'] },
};
const context = {
  userId: 'actor',
  holdingId: 'h001',
  companyId: 'company',
  branchId: null,
  departmentId: null,
  branchSlug: null,
  departmentSlug: null,
  assignmentId: 'assignment-actor',
  assignmentVersion: 3,
  revision: 7,
  role: 'owner',
  scopeKind: 'holding',
  modules: ['records'],
  permissions: ['records.read'],
  expiresAt: 9_007_199_254_740_991,
};
const linkRow = {
  id: 'link-1',
  sourceId: 'source-1',
  filename: 'evidence.pdf',
  mediaType: 'application/pdf',
  bytes: 12,
  sourceState: 'available',
  linkState: 'draft',
  caption: null,
  previewAvailable: true,
  createdAt: 1_000,
  expiresAt: 11_000,
  boundAt: null,
};
const setRow = {
  id: 'set-1',
  targetType: 'asset',
  targetId: 'asset-1',
  componentKind: 'record',
  componentRef: null,
  purposeKey: 'manual',
  expectedTargetVersion: 3,
  policyVersion: 1,
  classification: 'internal',
  requiredCount: 1,
  maxFiles: 10,
  state: 'draft',
  createdAt: 1_000,
  updatedAt: 1_100,
  links: [linkRow],
};
const statusRow = {
  linkId: 'link-1',
  setId: 'set-1',
  sourceId: 'source-1',
  sourceState: 'available',
  linkState: 'draft',
  scan: {
    attempts: 1,
    lastResult: 'clean',
    lastError: null,
    signature: 'Signature',
    definitionUpdatedAt: 1_000,
    scannedAt: 1_000,
  },
  previewAvailable: true,
  bindable: true,
  expiresAt: 11_000,
  boundAt: null,
  withdrawnAt: null,
};
const throwsCode = (action: () => unknown, code: string) => {
  try {
    action();
  } catch (error) {
    expect(error).toMatchObject({ code });
    return;
  }
  throw new Error(`Expected ${code}`);
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
const input = {
  holdingSlug: 'acme',
  targetType: 'asset',
  targetId: 'asset-1',
  purposeKey: 'manual',
  componentKind: 'record',
  expectedTargetVersion: 3,
  readOnly: false,
};

afterEach(() => {
  delete process.env.ERPX_NATIVE_ATTACHMENT_READ_CONFIG;
  delete process.env.ERPX_NATIVE_ATTACHMENT_COMMAND_CONFIG;
  vi.unstubAllEnvs();
});

describe('native Files attachment metadata transport', () => {
  it('forwards the exact set query, selected context and private headers', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: [setRow], context }));
    const client = createNativeAttachmentReadClient(parsed(), transport);
    const rows = await client.list(incoming(), actor, { ...input, readOnly: true });
    expect(rows).toEqual([setRow]);
    const [method, url, headers, body] = transport.mock.calls[0]!;
    expect(method).toBe('GET');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44993/api/v1/holdings/acme/attachment-sets?targetType=asset&targetId=asset-1&purposeKey=manual&componentKind=record&expectedTargetVersion=3&readOnly=true',
    );
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-assignment')).toBe('assignment-actor');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(headers.get('x-erpx-workspace-branch')).toBeNull();
    expect(headers.get('idempotency-key')).toBeNull();
    expect(headers.get('origin')).toBeNull();
    expect(body).toBeUndefined();
  });

  it('forwards the exact status path and complete scan evidence', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: statusRow, context }));
    const client = createNativeAttachmentReadClient(parsed(), transport);
    const status = await client.status(incoming(), actor, 'acme', 'link-1');
    expect(status).toEqual(statusRow);
    const [method, url] = transport.mock.calls[0]!;
    expect(method).toBe('GET');
    expect(url.toString()).toBe(
      'http://127.0.0.1:44993/api/v1/holdings/acme/attachment-links/link-1/status',
    );
  });

  it('rejects unknown query, wrong link identity and cross-Company response shapes', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: [setRow], context }));
    const client = createNativeAttachmentReadClient(parsed(), transport);
    await expect(
      client.list(incoming(), actor, { ...input, componentKind: 'document' }),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_READ_REQUEST_INVALID' });
    await expect(client.status(incoming(), actor, 'acme', 'link/other')).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_REQUEST_INVALID',
    });
    const wrongStatus = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockResolvedValue(ok({ data: { ...statusRow, linkId: 'link-other' }, context })),
    );
    await expect(wrongStatus.status(incoming(), actor, 'acme', 'link-1')).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_RESPONSE_INVALID',
    });
  });

  it('rejects malformed, oversized, non-JSON and context-changed responses', async () => {
    const extra = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockResolvedValue(ok({ data: [{ ...setRow, unexpected: true }], context })),
    );
    await expect(extra.list(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_RESPONSE_INVALID',
    });

    const reordered = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockResolvedValue(
        ok({
          data: [
            {
              ...setRow,
              links: [
                { ...linkRow, id: 'link-2', createdAt: 2_000 },
                { ...linkRow, id: 'link-1', createdAt: 1_000 },
              ],
            },
          ],
          context,
        }),
      ),
    );
    await expect(reordered.list(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_RESPONSE_INVALID',
    });

    const changedContext = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockResolvedValue(ok({ data: [setRow], context: { ...context, companyId: 'other' } })),
    );
    await expect(changedContext.list(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_CONTEXT_CHANGED',
    });

    const html = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockResolvedValue(
        new Response('<html></html>', { status: 200, headers: { 'content-type': 'text/html' } }),
      ),
    );
    await expect(html.list(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_RESPONSE_INVALID',
    });

    const oversized = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockResolvedValue(
        new Response('x'.repeat(2 * 1024 * 1024 + 1), {
          status: 200,
          headers: { 'content-type': 'application/json' },
        }),
      ),
    );
    await expect(oversized.list(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_RESPONSE_LIMIT',
    });
  });

  it('preserves bounded owner error codes and never falls back after a configured failure', async () => {
    const denied = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockResolvedValue(
        new Response(
          JSON.stringify({
            error: { code: 'ATTACHMENT_LINK_NOT_FOUND', message: 'File link not found.' },
          }),
          { status: 404, headers: { 'content-type': 'application/json' } },
        ),
      ),
    );
    await expect(denied.status(incoming(), actor, 'acme', 'link-1')).rejects.toMatchObject({
      code: 'ATTACHMENT_LINK_NOT_FOUND',
      status: 404,
    });
    expect(nativeAttachmentReadFailure(new NativeAttachmentReadError('ATTACHMENT_LINK_NOT_FOUND', 404))).toEqual({
      status: 404,
      code: 'ATTACHMENT_LINK_NOT_FOUND',
      message: 'The native file reader could not complete the request.',
    });

    const offline = createNativeAttachmentReadClient(
      parsed(),
      vi.fn().mockRejectedValue(new Error('connect ECONNREFUSED')),
    );
    await expect(offline.list(incoming(), actor, input)).rejects.toMatchObject({
      code: 'NATIVE_ATTACHMENT_READ_UNAVAILABLE',
    });

    const unknownHost = createNativeAttachmentReadClient(parsed(), vi.fn());
    await expect(
      unknownHost.list(incoming({ host: 'other.example.com' }), actor, input),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_READ_AUTHORITY_INVALID' });
  });

  it('validates the private configuration and fails closed on unsafe targets', () => {
    expect(nativeAttachmentReadAvailable()).toBe(false);
    vi.stubEnv('ERPX_NATIVE_ATTACHMENT_READ_CONFIG', config());
    expect(nativeAttachmentReadAvailable()).toBe(true);
    for (const privateBaseUrl of [
      'http://localhost:44993',
      'https://127.0.0.1:44993',
      'http://127.0.0.1:3100',
      'http://127.0.0.1:3200',
      'http://127.0.0.1:5432',
      'http://127.0.0.1:44993/path',
      'http://user:pass@127.0.0.1:44993',
    ]) {
      throwsCode(() => createNativeAttachmentReadClient(parsed({ privateBaseUrl }), vi.fn()), 'NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
    }
    for (const holdings of [
      [{ origin: 'http://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' }],
      [{ origin: 'https://acme.example.com:8443', holdingSlug: 'acme', holdingId: 'h001' }],
      [{ origin: 'https://acme.example.com/path', holdingSlug: 'acme', holdingId: 'h001' }],
      [{ origin: 'https://acme.example.com', holdingSlug: 'Acme', holdingId: 'h001' }],
      [
        { origin: 'https://acme.example.com', holdingSlug: 'acme', holdingId: 'h001' },
        { origin: 'https://acme.example.com', holdingSlug: 'other', holdingId: 'h002' },
      ],
    ]) {
      throwsCode(() => createNativeAttachmentReadClient(parsed({ holdings }), vi.fn()), 'NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID');
    }
    throwsCode(
      () => nativeAttachmentReadConfiguration('{'),
      'NATIVE_ATTACHMENT_READ_CONFIGURATION_INVALID',
    );
  });
});

describe('native Files attachment command transport', () => {
  const withdrawnRow = {
    ...statusRow,
    linkState: 'withdrawn',
    bindable: false,
    withdrawnAt: 5_000,
  };
  const commandIncoming = () =>
    new Headers({
      host: 'acme.example.com',
      origin: 'https://acme.example.com',
      cookie: '__Host-erpx-product=session',
    });

  it('forwards the exact reason, key, origin and selected context', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: withdrawnRow, context }));
    const client = createNativeAttachmentCommandClient(parsed(), transport);
    const row = await client.withdraw(
      commandIncoming(),
      actor,
      'acme',
      'link-1',
      { reason: '  duplicate upload  ', key: 'withdraw-key-1' },
    );
    expect(row).toEqual(withdrawnRow);
    const [url, headers, body] = transport.mock.calls[0]!;
    expect(url.toString()).toBe(
      'http://127.0.0.1:44993/api/v1/holdings/acme/attachment-links/link-1/withdraw',
    );
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('origin')).toBe('https://acme.example.com');
    expect(headers.get('content-type')).toBe('application/json');
    expect(headers.get('idempotency-key')).toBe('withdraw-key-1');
    expect(headers.get('x-erpx-context-company')).toBe('company');
    expect(headers.get('x-erpx-context-assignment')).toBe('assignment-actor');
    expect(headers.get('x-erpx-context-revision')).toBe('7');
    expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
    expect(JSON.parse(body)).toEqual({ reason: 'duplicate upload' });
  });

  it('rejects a non-withdrawn reply, a changed context and a missing origin', async () => {
    const draft = createNativeAttachmentCommandClient(
      parsed(),
      vi.fn().mockResolvedValue(ok({ data: statusRow, context })),
    );
    await expect(
      draft.withdraw(commandIncoming(), actor, 'acme', 'link-1', {
        reason: 'duplicate upload',
        key: 'withdraw-key-1',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_COMMAND_RESPONSE_INVALID' });

    const changed = createNativeAttachmentCommandClient(
      parsed(),
      vi.fn().mockResolvedValue(ok({
        data: withdrawnRow,
        context: { ...context, assignmentVersion: 4 },
      })),
    );
    await expect(
      changed.withdraw(commandIncoming(), actor, 'acme', 'link-1', {
        reason: 'duplicate upload',
        key: 'withdraw-key-1',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_READ_CONTEXT_CHANGED' });

    const noOrigin = createNativeAttachmentCommandClient(parsed(), vi.fn());
    await expect(
      noOrigin.withdraw(
        new Headers({ host: 'acme.example.com', cookie: '__Host-erpx-product=session' }),
        actor,
        'acme',
        'link-1',
        { reason: 'duplicate upload', key: 'withdraw-key-1' },
      ),
    ).rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN', status: 403 });
  });

  it('maps configured writes to a bounded unavailable error and fails closed', async () => {
    expect(nativeAttachmentCommandAvailable()).toBe(false);
    vi.stubEnv('ERPX_NATIVE_ATTACHMENT_COMMAND_CONFIG', config());
    expect(nativeAttachmentCommandAvailable()).toBe(true);
    throwsCode(
      () => nativeAttachmentCommandConfiguration('{'),
      'NATIVE_ATTACHMENT_COMMAND_CONFIGURATION_INVALID',
    );
    throwsCode(
      () =>
        nativeAttachmentCommandConfiguration(
          JSON.stringify({ privateBaseUrl: 'http://127.0.0.1:44993', timeoutMs: 1000, holdings: [] }),
        ),
      'NATIVE_ATTACHMENT_COMMAND_CONFIGURATION_INVALID',
    );
    const offline = createNativeAttachmentCommandClient(
      parsed(),
      vi.fn().mockRejectedValue(new Error('connect ECONNREFUSED')),
    );
    await expect(
      offline.withdraw(commandIncoming(), actor, 'acme', 'link-1', {
        reason: 'duplicate upload',
        key: 'withdraw-key-1',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_COMMAND_UNAVAILABLE', status: 503 });
    expect(
      nativeAttachmentCommandFailure(
        new NativeAttachmentReadError('NATIVE_ATTACHMENT_COMMAND_UNAVAILABLE', 503),
      ),
    ).toEqual({
      status: 503,
      code: 'NATIVE_ATTACHMENT_WRITER_UNAVAILABLE',
      message: 'The native file writer is unavailable.',
    });
  });
});


describe('native Customer portal Files command transport', () => {
  const withdrawnRow = {
    ...statusRow,
    linkState: 'withdrawn',
    bindable: false,
    withdrawnAt: 5_000,
  };
  const portalIncoming = () =>
    new Headers({
      host: 'acme.example.com',
      origin: 'https://acme.example.com',
      cookie: '__Host-erpx-product=portal-session',
    });

  it('forwards only the portal cookie, origin, key and bounded reason', async () => {
    const transport = vi.fn().mockResolvedValue(ok({ data: withdrawnRow }));
    const client = createNativePortalAttachmentCommandClient(parsed(), transport);
    const row = await client.withdraw(portalIncoming(), 'acme', 'link-1', {
      reason: '  duplicate upload  ',
      key: 'withdraw-key-1',
    });
    expect(row).toEqual(withdrawnRow);
    const [url, headers, body] = transport.mock.calls[0]!;
    expect(url.toString()).toBe(
      'http://127.0.0.1:44993/api/customer-portal/acme/attachments/attachment-links/link-1/withdraw',
    );
    expect(headers.get('host')).toBe('acme.example.com');
    expect(headers.get('origin')).toBe('https://acme.example.com');
    expect(headers.get('idempotency-key')).toBe('withdraw-key-1');
    expect(headers.get('cookie')).toBe('__Host-erpx-product=portal-session');
    expect(headers.get('x-erpx-context-company')).toBeNull();
    expect(JSON.parse(body)).toEqual({ reason: 'duplicate upload' });
  });

  it('fails closed on an unknown target, missing origin, invalid receipt and transport loss', async () => {
    const client = (transport: (url: URL, headers: Headers, body: string, signal: AbortSignal) => Promise<Response>) =>
      createNativePortalAttachmentCommandClient(parsed(), transport);
    await expect(
      client(vi.fn()).withdraw(portalIncoming(), 'other', 'link-1', {
        reason: 'duplicate upload',
        key: 'withdraw-key-1',
      }),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_COMMAND_AUTHORITY_INVALID', status: 403 });
    const noOrigin = portalIncoming();
    noOrigin.delete('origin');
    await expect(
      client(vi.fn()).withdraw(noOrigin, 'acme', 'link-1', {
        reason: 'duplicate upload',
        key: 'withdraw-key-1',
      }),
    ).rejects.toMatchObject({ code: 'ORIGIN_FORBIDDEN', status: 403 });
    await expect(
      client(vi.fn().mockResolvedValue(ok({ data: statusRow }))).withdraw(
        portalIncoming(),
        'acme',
        'link-1',
        { reason: 'duplicate upload', key: 'withdraw-key-1' },
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_COMMAND_RESPONSE_INVALID' });
    await expect(
      client(vi.fn().mockRejectedValue(new Error('connect ECONNREFUSED'))).withdraw(
        portalIncoming(),
        'acme',
        'link-1',
        { reason: 'duplicate upload', key: 'withdraw-key-1' },
      ),
    ).rejects.toMatchObject({ code: 'NATIVE_ATTACHMENT_COMMAND_UNAVAILABLE', status: 503 });
  });
});
