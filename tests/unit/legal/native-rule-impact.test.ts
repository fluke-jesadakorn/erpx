import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { createServer, type IncomingMessage, type Server } from 'node:http';

vi.mock('server-only', () => ({}));

import {
  nativeRuleImpactRead,
  type NativeRuleImpactActor,
  type NativeRuleImpactAuthority,
  type NativeRuleImpactRead,
} from '../../../packages/server-legal/src/adapters/native-rule-impact/client';

const ENV = 'ERPX_NATIVE_LEGAL_RULE_IMPACT_CONFIG';
const HOLDINGS = [
  {
    origin: 'https://acme.jesadakorn.com',
    host: 'acme.jesadakorn.com',
    holdingId: 'h001',
    holdingSlug: 'acme',
    companyId: 'acme-co',
  },
] as const;

const READ: NativeRuleImpactRead = {
  rule_id: 'rule-1',
  rule_version: 4,
  items: [
    {
      id: 'item-1',
      kind: 'contract',
      title: 'Agreement',
      state: 'draft',
      due_date: '2026-12-01',
      version: 2,
    },
  ],
  notice:
    'Only accessible linked records are shown. Existing deadlines remain unchanged until their owner reviews and updates each record.',
};

const actor: NativeRuleImpactActor = {
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: 'acme-co',
  userId: 'user-1',
  branchId: 'branch-1',
};
const authority: NativeRuleImpactAuthority = { companyEnabled: true, rulePurchased: true };

type Seen = { url: string; enabled: string | undefined; purchased: string | undefined; branch: string | undefined };
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
      enabled: req.headers['x-erpx-legal-rule-impact-enabled'] as string | undefined,
      purchased: req.headers['x-erpx-legal-rule-impact-purchased'] as string | undefined,
      branch: req.headers['x-erpx-context-branch'] as string | undefined,
    });
    if (mode === 'down') return respond(res, { error: { code: 'DOWN', message: 'down' } }, 503);
    if (mode === 'bad') return respond(res, { data: { ...READ, items: [{ id: 'only-id' }] } });
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

describe('native Legal rule-impact read', () => {
  it('returns the complete owner payload with exact authority and scope headers', async () => {
    const read = await nativeRuleImpactRead(actor, authority, 'rule-1');
    expect(read).toEqual(READ);
    expect(requests).toHaveLength(1);
    expect(requests[0].url).toBe('/api/v1/holdings/acme/legal/rule-impact/rule-1');
    expect(requests[0].enabled).toBe('true');
    expect(requests[0].purchased).toBe('true');
    expect(requests[0].branch).toBe('branch-1');
  });

  it('fails closed on a malformed payload with no TypeScript fallback', async () => {
    mode = 'bad';
    await expect(nativeRuleImpactRead(actor, authority, 'rule-1')).rejects.toMatchObject({
      code: 'NATIVE_LEGAL_RULE_IMPACT_RESPONSE_INVALID',
    });
  });

  it('surfaces owner 4xx codes verbatim and treats 5xx as unavailable', async () => {
    mode = 'down';
    await expect(nativeRuleImpactRead(actor, authority, 'rule-1')).rejects.toMatchObject({
      code: 'NATIVE_LEGAL_RULE_IMPACT_UNAVAILABLE',
    });
  });

  it('rejects an unregistered actor before any request', async () => {
    await expect(
      nativeRuleImpactRead({ ...actor, holdingSlug: 'unknown' }, authority, 'rule-1'),
    ).rejects.toMatchObject({ code: 'NATIVE_LEGAL_RULE_IMPACT_AUTHORITY_INVALID' });
    expect(requests).toHaveLength(0);
  });

  it('rejects an unbounded id before any request', async () => {
    await expect(nativeRuleImpactRead(actor, authority, 'bad id!')).rejects.toMatchObject({
      code: 'NATIVE_LEGAL_RULE_IMPACT_REQUEST_INVALID',
    });
    expect(requests).toHaveLength(0);
  });
});
