import { afterEach, beforeEach, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

import {
  nativeDirectoryOperationsCancel,
  nativeDirectoryOperationsConfigure,
  nativeDirectoryOperationsPreview,
  nativeDirectoryOperationsRead,
  nativeDirectoryOperationsRefresh,
  nativeDirectoryOperationsRetry,
  nativeDirectoryOperationsReview,
  nativeDirectoryOperationsSubmit,
} from '@erpx/server-identity_access/adapters/native-directory-operations';

/**
 * Native Directory automation client parity.
 *
 * The command owner, transaction and provider readback assertions now live in
 * the Rust owner (`rust-directory-operation` and the identity-access gateway).
 * This suite retains the exact Next transport wire contract: private base,
 * registered Holding host, context forwarding and Idempotency-Key placement.
 */
const CONFIGURATION = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:3310',
  timeoutMs: 5_000,
  holdings: [
    {
      origin: 'https://acme.jesadakorn.com',
      holdingSlug: 'acme',
      holdingId: 'H001',
    },
  ],
});

const headers = () =>
  new Headers({
    host: 'acme.jesadakorn.com',
    origin: 'https://acme.jesadakorn.com',
    cookie: 'erpx_session=secret',
    'x-erpx-context-company': 'acme',
  });

let calls: { url: string; method: string; headers: Headers; body: unknown }[];

beforeEach(() => {
  process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG = CONFIGURATION;
  calls = [];
  vi.stubGlobal('fetch', async (url: string, init: RequestInit) => {
    calls.push({
      url,
      method: String(init.method),
      headers: new Headers(init.headers),
      body: init.body === undefined ? undefined : JSON.parse(String(init.body)),
    });
    return new Response(JSON.stringify({ data: { ok: true } }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete process.env.ERPX_NATIVE_IDENTITY_ACCESS_CONFIG;
});

it('reads the automation workspace over the private Holding target', async () => {
  const data = await nativeDirectoryOperationsRead(headers(), 'acme');
  expect(data).toEqual({ ok: true });
  expect(calls).toHaveLength(1);
  expect(calls[0].url).toBe(
    'http://127.0.0.1:3310/api/v1/holdings/acme/iam/directory-operations',
  );
  expect(calls[0].method).toBe('GET');
  expect(calls[0].headers.get('host')).toBe('acme.jesadakorn.com');
  expect(calls[0].headers.get('cookie')).toBe('erpx_session=secret');
  expect(calls[0].headers.get('x-erpx-context-company')).toBe('acme');
});

it('forwards every command body and idempotency key exactly once', async () => {
  await nativeDirectoryOperationsRefresh(
    headers(),
    'acme',
    { connectionId: 'conn', expectedVersion: 2 },
    'key-refresh',
  );
  await nativeDirectoryOperationsConfigure(
    headers(),
    'acme',
    { connectionId: 'conn', expectedVersion: 2, writeEnabled: true },
    'key-configure',
  );
  await nativeDirectoryOperationsPreview(headers(), 'acme', {
    connectionId: 'conn',
    expectedVersion: 2,
  });
  await nativeDirectoryOperationsSubmit(
    headers(),
    'acme',
    { connectionId: 'conn', previewToken: 'token' },
    'key-submit',
  );
  await nativeDirectoryOperationsReview(
    headers(),
    'acme',
    { operationId: 'op', decision: 'approve' },
    'key-review',
  );
  await nativeDirectoryOperationsRetry(
    headers(),
    'acme',
    { id: 'op', expectedVersion: 1 },
    'key-retry',
  );
  await nativeDirectoryOperationsCancel(
    headers(),
    'acme',
    { id: 'op', expectedVersion: 1 },
    'key-cancel',
  );
  expect(calls.map((call) => call.url.split('/iam/directory-operations')[1])).toEqual([
    '/refresh',
    '/configure',
    '/preview',
    '/submit',
    '/reviews/apply',
    '/retry',
    '/cancel',
  ]);
  expect(calls.map((call) => call.headers.get('idempotency-key'))).toEqual([
    'key-refresh',
    'key-configure',
    null,
    'key-submit',
    'key-review',
    'key-retry',
    'key-cancel',
  ]);
  expect(calls[3].body).toEqual({ connectionId: 'conn', previewToken: 'token' });
});

it('refuses an unregistered Holding host instead of calling the private service', async () => {
  const foreign = new Headers({ host: 'other.jesadakorn.com' });
  let failure: { status?: number; code?: string } | undefined;
  try {
    await nativeDirectoryOperationsRead(foreign, 'acme');
  } catch (error) {
    failure = error as { status?: number; code?: string };
  }
  expect(failure?.status).toBe(503);
  expect(failure?.code).toBe('NATIVE_DIRECTORY_OPERATION_UNAVAILABLE');
  expect(calls).toHaveLength(0);
});
