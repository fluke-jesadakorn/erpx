import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { createServer, type IncomingMessage, type Server } from 'node:http';

vi.mock('server-only', () => ({}));

import {
  nativeProcessingStatusRead,
  type NativeProcessingStatusActor,
  type NativeProcessingStatusAuthority,
  type NativeProcessingStatusRead,
} from '../../../packages/server-legal/src/adapters/native-processing-status/client';

const ENV = 'ERPX_NATIVE_LEGAL_PROCESSING_STATUS_CONFIG';
const HOLDINGS = [
  {
    origin: 'https://acme.jesadakorn.com',
    host: 'acme.jesadakorn.com',
    holdingId: 'h001',
    holdingSlug: 'acme',
    companyId: 'acme-co',
  },
] as const;

const READ: NativeProcessingStatusRead = {
  id: 'job-1',
  item_id: 'item-1',
  document_id: 'doc-1',
  state: 'queued',
  error: null,
  attempts: 2,
  jobId: 'job-1',
  result: { state: 'queued' },
};

const actor: NativeProcessingStatusActor = {
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'acme-co',
};
const authority: NativeProcessingStatusAuthority = { itemAccessible: true };

type Seen = { url: string; company: string | undefined; accessible: string | undefined };
let server: Server;
let requests: Seen[];
let mode: 'ok' | 'bad' | 'down' = 'ok';
let original: string | undefined;

const respond = (res: import('node:http').ServerResponse, body: unknown, status = 200) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(body));
};

beforeAll(async () => {
  requests = [];
  server = createServer((req: IncomingMessage, res) => {
    const url = new URL(req.url ?? '/', 'http://127.0.0.1');
    requests.push({
      url: url.pathname,
      company: req.headers['x-erpx-context-company'] as string | undefined,
      accessible: req.headers['x-erpx-legal-processing-accessible'] as string | undefined,
    });
    if (mode === 'down') return respond(res, { error: { code: 'DOWN', message: 'down' } }, 503);
    if (mode === 'bad') return respond(res, { data: { ...READ, jobId: undefined } });
    return respond(res, { data: READ });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
});

afterAll(async () => {
  await new Promise<void>(resolve => server.close(() => resolve()));
});

beforeEach(() => {
  mode = 'ok';
  requests = [];
  original = process.env[ENV];
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no server port');
  process.env[ENV] = JSON.stringify({
    privateBaseUrl: `http://127.0.0.1:${address.port}`,
    timeoutMs: 1000,
    holdings: HOLDINGS,
  });
});

afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
});

describe('native Legal processing-status read', () => {
  it('returns the complete owner payload with exact authority and scope headers', async () => {
    const read = await nativeProcessingStatusRead(actor, authority, 'job-1');
    expect(read).toEqual(READ);
    expect(requests).toHaveLength(1);
    expect(requests[0].url).toBe('/api/v1/holdings/acme/legal/processing/job-1');
    expect(requests[0].company).toBe('acme-co');
    expect(requests[0].accessible).toBe('true');
  });

  it('passes a false item-access witness verbatim so the owner can deny', async () => {
    await expect(
      nativeProcessingStatusRead(actor, { itemAccessible: false }, 'job-1'),
    ).resolves.toEqual(READ);
    expect(requests[0].accessible).toBe('false');
  });

  it('fails closed when no native configuration is present', async () => {
    delete process.env[ENV];
    await expect(nativeProcessingStatusRead(actor, authority, 'job-1')).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_PROCESSING_STATUS_CONFIGURATION_REQUIRED',
    });
    expect(requests).toHaveLength(0);
  });

  it('fails closed on a malformed payload with no TypeScript fallback', async () => {
    mode = 'bad';
    await expect(nativeProcessingStatusRead(actor, authority, 'job-1')).rejects.toMatchObject({
      code: 'NATIVE_LEGAL_PROCESSING_STATUS_RESPONSE_INVALID',
    });
  });

  it('surfaces owner 4xx codes verbatim and treats 5xx as unavailable', async () => {
    mode = 'down';
    await expect(nativeProcessingStatusRead(actor, authority, 'job-1')).rejects.toMatchObject({
      code: 'NATIVE_LEGAL_PROCESSING_STATUS_UNAVAILABLE',
    });
  });

  it('rejects an unregistered actor before any request', async () => {
    await expect(
      nativeProcessingStatusRead({ ...actor, holdingSlug: 'unknown' }, authority, 'job-1'),
    ).rejects.toMatchObject({ code: 'NATIVE_LEGAL_PROCESSING_STATUS_AUTHORITY_INVALID' });
    expect(requests).toHaveLength(0);
  });

  it('rejects an unbounded id before any request', async () => {
    await expect(nativeProcessingStatusRead(actor, authority, 'bad id!')).rejects.toMatchObject({
      code: 'NATIVE_LEGAL_PROCESSING_STATUS_REQUEST_INVALID',
    });
    expect(requests).toHaveLength(0);
  });
});
