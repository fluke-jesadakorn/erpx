import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const root = resolve(__dirname, '../../..');
const directory =
  'packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/records';
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

// The four registry routes whose authority moved to the Rust axum registry.
const routes = [
  `${directory}/[module]/route.ts`,
  `${directory}/[module]/[id]/route.ts`,
  `${directory}/[module]/[id]/references/route.ts`,
  `${directory}/[module]/[id]/transition/route.ts`,
] as const;

// These three have no native axum target and keep their HEAD TS owner implementation.
const retained = [
  [`${directory}/[module]/[id]/mentions/route.ts`, 'addPrincipalMention'],
  [`${directory}/[module]/[id]/suggestions/route.ts`, 'requestSuggestion'],
  [`${directory}/[module]/[id]/suggestions/[suggestionId]/route.ts`, 'decideSuggestion'],
] as const;

const calls = vi.hoisted(() => ({ proxy: [] as unknown[][] }));

vi.mock('@erpx/server-records/public/native-record-registry', () => ({
  proxyNativeRecordRegistry: vi.fn(async (...args: unknown[]) => {
    calls.proxy.push(args);
    return new Response(JSON.stringify({ data: {}, context: {} }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    });
  }),
}));

import { GET, POST } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/route';
import {
  DELETE,
  GET as GET_RECORD,
  PATCH,
} from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/route';
import { POST as POST_REFERENCE } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/references/route';
import { POST as POST_TRANSITION } from '@erpx/workplace/routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/transition/route';

const request = () =>
  new Request('https://acme.jesadakorn.com/api/v1/holdings/acme/records/operations?section=overview', {
    method: 'POST',
    headers: { host: 'acme.jesadakorn.com', 'content-type': 'application/json' },
    body: '{}',
  });
const context = (extra: Record<string, string> = {}) =>
  ({ params: Promise.resolve({ holdingSlug: 'acme', module: 'operations', id: 'rec-1', suggestionId: 's-1', ...extra }) }) as never;

beforeEach(() => {
  calls.proxy.length = 0;
});

describe('registry routes are pure axum transport', () => {
  it('contains no owner SQL, schema, work-context or payload reshaping', () => {
    const forbidden = [
      /SELECT\s/i,
      /INSERT\s+INTO/i,
      /UPDATE\s+\w+\s+SET/i,
      /DELETE\s+FROM/i,
      /\.prepare\(/,
      'requireWorkContext',
      'canonicalDestinationLinks',
      'safeParse',
      '@erpx/server-records/public/records',
      '@erpx/server-records/adapters/records',
      'createNativeRecordRegistryClient',
      'createNativeRecordContextClient',
      'nativeRecordRegistryAvailable',
      'requestPublicOrigin',
      'errorResponse',
      'ApiError',
    ];
    for (const path of routes) {
      const source = read(path);
      for (const token of forbidden) {
        expect(source, `${path} contains ${String(token)}`).not.toMatch(
          typeof token === 'string' ? new RegExp(token.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')) : token,
        );
      }
      expect(source).toContain('@erpx/server-records/public/native-record-registry');
    }
  });

  it('delegates each native handler to one exact axum call', async () => {
    const req = request();
    await GET(req, context());
    await POST(req, context());
    await GET_RECORD(req, context());
    await PATCH(req, context());
    await DELETE(req, context());
    await POST_REFERENCE(req, context());
    await POST_TRANSITION(req, context());
    expect(calls.proxy).toEqual([
      [req, 'acme', { method: 'GET', parts: ['operations'], query: true }],
      [req, 'acme', { method: 'POST', parts: ['operations'], body: true }],
      [req, 'acme', { method: 'GET', parts: ['operations', 'rec-1'] }],
      [req, 'acme', { method: 'PATCH', parts: ['operations', 'rec-1'], body: true }],
      [req, 'acme', { method: 'DELETE', parts: ['operations', 'rec-1'], query: true }],
      [req, 'acme', { method: 'POST', parts: ['operations', 'rec-1', 'references'], body: true }],
      [req, 'acme', { method: 'POST', parts: ['operations', 'rec-1', 'transition'], body: true }],
    ]);
  });
});

describe('routes without a native target keep their HEAD TS implementation', () => {
  it('calls the owner through requireWorkContext instead of failing closed', () => {
    for (const [path, fn] of retained) {
      const source = read(path);
      expect(source, path).toContain('requireWorkContext');
      expect(source, path).toContain('canonicalDestinationLinks');
      expect(source, path).toContain(fn);
      expect(source, path).toContain('@erpx/server-records/public/records');
      expect(source, path).not.toContain('nativeRecordRegistryUnsupported');
    }
  });
});

describe('search route is pure native transport', () => {
  it('calls the native owner transport with no TS SQL fallback', () => {
    const source = read(`${directory}/search/route.ts`);
    expect(source).toContain('@erpx/server-records/public/native-record-search');
    expect(source).toContain('nativeRecordSearch');
    expect(source).toContain('requireWorkContext');
    expect(source).toContain('canonicalDestinationLinks');
    expect(source).not.toContain('searchReferences');
    expect(source).not.toContain('@erpx/server-records/public/records');
    expect(source).not.toContain('@erpx/server-records/adapters/records');
    expect(source).not.toMatch(/SELECT\s/i);
    expect(source).not.toContain('.prepare(');
  });
});
