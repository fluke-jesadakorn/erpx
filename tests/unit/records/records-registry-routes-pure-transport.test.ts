import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';

const root = resolve(__dirname, '../../..');
const directory =
  'packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/records';
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

const registryPaths = [
  '[module]/route',
  '[module]/[id]/route',
  '[module]/[id]/references/route',
  '[module]/[id]/transition/route',
] as const;

// These three have no native axum target and keep their HEAD TS owner implementation.
const retained = [
  [`${directory}/[module]/[id]/mentions/route.ts`, 'addPrincipalMention'],
  [`${directory}/[module]/[id]/suggestions/route.ts`, 'requestSuggestion'],
  [`${directory}/[module]/[id]/suggestions/[suggestionId]/route.ts`, 'decideSuggestion'],
] as const;

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
    return source.slice(start, end).replace(/\s+/g, ' ').replace(/\s+\./g, '.');
  };

  it('registers exactly the four axum path families with the delegated methods', () => {
    const axum = axumRouter();
    expect(axum.match(/\.route\(/g)).toHaveLength(4);
    for (const [path, methods] of [
      ['/api/v1/holdings/{holding_slug}/records/{module_id}', 'get(read_registry).post(create_record).options(collection_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}', 'patch(update_record).delete(delete_record).options(record_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/references', 'post(add_reference).options(command_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/transition', 'post(transition_record).options(command_options)'],
    ] as const)
      expect(axum, `${path} method surface`).toContain(`"${path}", ${methods}`);
  });

  it('keeps HEAD derived by the native Records context route', () => {
    const context = read('packages/server-records/src/adapters/record_context_http.rs');
    expect(context).toContain('get(read_context)');
    expect(context).not.toContain('.head(');
  });

  it('retired route files and package exports cannot resolve', () => {
    const exports = JSON.parse(read('packages/workplace/package.json')).exports as Record<string, string>;
    for (const path of registryPaths) {
      expect(existsSync(resolve(root, directory, path + '.ts')), path).toBe(false);
      expect(existsSync(resolve(root, 'apps/compat/src/app/api/v1/holdings/[holdingSlug]/records', path + '.ts')), path).toBe(false);
      expect(exports['./routes/api/v1/holdings/[holdingSlug]/records/' + path]).toBeUndefined();
    }
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
