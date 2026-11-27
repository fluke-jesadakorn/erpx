import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';

const root = resolve(__dirname, '../../..');
const directory =
  'packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/records';
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

// The three retained registry routes whose authority moved to the Rust axum registry.
const routes = [
  `${directory}/[module]/[id]/route.ts`,
  `${directory}/[module]/[id]/references/route.ts`,
  `${directory}/[module]/[id]/transition/route.ts`,
] as const;

// Collection GET/POST transport is retired; the browser reaches axum directly.
const retired = [
  `${directory}/[module]/route.ts`,
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/records/[module]/route.ts',
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
    await GET_RECORD(req, context());
    await PATCH(req, context());
    await DELETE(req, context());
    await POST_REFERENCE(req, context());
    await POST_TRANSITION(req, context());
    expect(calls.proxy).toEqual([
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

describe('axum mount and method-surface parity for the four delegated families', () => {
  const axumRouter = () => {
    const source = read('packages/server-records/src/adapters/record_registry_http.rs');
    const start = source.indexOf('pub fn records_registry_http_router');
    const end = source.indexOf('with_state(Arc::new(RecordsRegistryHttpState', start);
    return source.slice(start, end).replace(/\s+/g, ' ');
  };

  it('registers exactly the four axum path families with the delegated methods', () => {
    const axum = axumRouter();
    expect(axum.match(/\.route\(/g)).toHaveLength(4);
    for (const [path, methods] of [
      ['/api/v1/holdings/{holding_slug}/records/{module_id}', 'get(read_registry).post(create_record).options(collection_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}', 'patch(update_record).delete(delete_record)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/references', 'post(add_reference)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/transition', 'post(transition_record)'],
    ] as const)
      expect(axum, `${path} method surface`).toContain(`"${path}", ${methods}`);
  });

  it('keeps HEAD derived on the retained Next routes and on axum', () => {
    for (const path of routes) {
      const source = read(path);
      expect(source, `${path} must not add a HEAD handler`).not.toMatch(/export\s+(?:async\s+)?function\s+HEAD\b/);
      expect(source, `${path} must not add an OPTIONS handler`).not.toMatch(/export\s+(?:async\s+)?function\s+OPTIONS\b/);
    }
    const axum = axumRouter();
    expect(axum).not.toContain('head(');
  });

  it('pins the retired collection OPTIONS to 204 with the Next Allow surface', () => {
    const axum = axumRouter();
    expect(axum.match(/\.options\(/g)).toHaveLength(1);
    expect(axum).toContain('get(read_registry).post(create_record).options(collection_options)');
    const source = read('packages/server-records/src/adapters/record_registry_http.rs');
    const start = source.indexOf('async fn collection_options');
    expect(start).toBeGreaterThan(-1);
    const handler = source.slice(start, source.indexOf('\n}', start));
    expect(handler).toContain('StatusCode::NO_CONTENT');
    expect(handler).toContain('header::ALLOW');
    expect(handler).toContain('"GET, HEAD, OPTIONS, POST"');
  });

  it('retired collection route files and package export cannot resolve', () => {
    for (const path of retired) expect(existsSync(resolve(root, path)), path).toBe(false);
    const exports = JSON.parse(read('packages/workplace/package.json')).exports as Record<string, string>;
    expect(exports['./routes/api/v1/holdings/[holdingSlug]/records/[module]/route']).toBeUndefined();
    expect(exports['./routes/api/v1/holdings/[holdingSlug]/records/[module]/[id]/route']).toBeDefined();
  });
});

describe('exact literal module selector preserves /records/search', () => {
  const host = 'acme.jesadakorn.com';
  const reserved = [
    '/api/v1/holdings/acme/records/search?q=a',
    '/api/v1/holdings/acme/records/native-search?q=a',
  ];
  const wildcard = {
    NODE_ENV: 'test' as const,
    ERPX_DEV_NATIVE_PATHS: `${host}|GET|/api/v1/holdings/{holding}/records/{module}`,
  };
  const modules = ['operations', 'support', 'case'];
  const exact = {
    NODE_ENV: 'test' as const,
    ERPX_DEV_NATIVE_PATHS: modules
      .flatMap((module) => [
        `${host}|GET|/api/v1/holdings/acme/records/${module}`,
        `${host}|POST|/api/v1/holdings/acme/records/${module}`,
        `${host}|GET|/api/v1/holdings/acme/records/${module}/{id}`,
        `${host}|PATCH|/api/v1/holdings/acme/records/${module}/{id}`,
        `${host}|DELETE|/api/v1/holdings/acme/records/${module}/{id}`,
        `${host}|POST|/api/v1/holdings/acme/records/${module}/{id}/references`,
        `${host}|POST|/api/v1/holdings/acme/records/${module}/{id}/transition`,
      ])
      .join(','),
  };

  it('shows the wildcard module template also swallows the search paths', () => {
    for (const path of reserved)
      expect(nativeApiPath(wildcard, path, host, 'GET'), `${path} must expose the wildcard hazard`).toBe(true);
  });

  it('keeps all four delegated families per exact module and excludes search', () => {
    for (const module of modules)
      for (const [suffix, method] of [
        ['', 'GET'],
        ['', 'POST'],
        ['/rec-1', 'GET'],
        ['/rec-1', 'PATCH'],
        ['/rec-1', 'DELETE'],
        ['/rec-1/references', 'POST'],
        ['/rec-1/transition', 'POST'],
      ] as const)
        expect(
          nativeApiPath(exact, `/api/v1/holdings/acme/records/${module}${suffix}?section=overview`, host, method),
          `${method} /records/${module}${suffix}`,
        ).toBe(true);
    for (const path of reserved)
      expect(nativeApiPath(exact, path, host, 'GET'), `${path} must stay on the Next search route`).toBe(false);
    expect(nativeApiPath(exact, '/api/v1/holdings/acme/records/operations/rec-1/suggestions', host, 'POST')).toBe(false);
  });

  it('does not rely on the Rust module validator to reserve search literals', () => {
    const context = read('packages/server-records/src/domain/record_context.rs');
    const body = context.slice(
      context.indexOf('pub(crate) fn valid_module'),
      context.indexOf('pub(crate) fn valid_id'),
    );
    expect(body).toContain('value.bytes().all');
    expect(body).not.toContain('search');
  });
});
