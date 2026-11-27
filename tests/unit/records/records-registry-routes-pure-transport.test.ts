import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { holdingHosts } from '@erpx/contracts/holding-hosts';
import { describe, expect, it } from 'vitest';
import { expandNativeApiPaths, loadNativeApiPaths } from '../../../scripts/local/native-api-paths';
import { nativeApiPath, nativeApiRewrite } from '../../../scripts/local/native-api-proxy';

const root = resolve(__dirname, '../../..');
const directory =
  'packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/records';
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');

const registryPaths = [
  '[module]/route',
  '[module]/[id]/route',
  '[module]/[id]/references/route',
  '[module]/[id]/mentions/route',
  '[module]/[id]/suggestions/route',
  '[module]/[id]/suggestions/[suggestionId]/route',
  '[module]/[id]/transition/route',
] as const;

describe('search route is retired native transport', () => {
  it('has no Next route; the native owner serves the rewritten path', () => {
    expect(existsSync(resolve(root, directory, 'search/route.ts'))).toBe(false);
    const http = read('packages/server-records/src/adapters/record_search_http.rs');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/records/native-search');
    expect(http).toContain('backend.search(&host, &headers, &query)');
  });
});

describe('axum mount and method-surface parity for the seven delegated families', () => {
  const axumRouter = () => {
    const source = read('packages/server-records/src/adapters/record_registry_http.rs');
    const start = source.indexOf('pub fn records_registry_http_router');
    const end = source.indexOf('with_state(Arc::new(RecordsRegistryHttpState', start);
    return source.slice(start, end).replace(/\s+/g, ' ').replace(/\s+\./g, '.');
  };

  it('registers exactly the seven axum path families with the delegated methods', () => {
    const axum = axumRouter();
    expect(axum.match(/\.route\(/g)).toHaveLength(7);
    for (const [path, methods] of [
      ['/api/v1/holdings/{holding_slug}/records/{module_id}', 'get(read_registry).post(create_record).options(collection_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}', 'patch(update_record).delete(delete_record).options(record_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/references', 'post(add_reference).options(command_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/mentions', 'any(|| async { StatusCode::METHOD_NOT_ALLOWED }).post(add_mention).options(command_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/suggestions', 'any(|| async { StatusCode::METHOD_NOT_ALLOWED }).post(add_suggestion).options(command_options)'],
      ['/api/v1/holdings/{holding_slug}/records/{module_id}/{record_id}/suggestions/{suggestion_id}', 'any(|| async { StatusCode::METHOD_NOT_ALLOWED }).post(decide_suggestion).options(command_options)'],
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
        `${host}|POST|/api/v1/holdings/acme/records/${module}/{id}/mentions`,
        `${host}|POST|/api/v1/holdings/acme/records/${module}/{id}/suggestions`,
        `${host}|POST|/api/v1/holdings/acme/records/${module}/{id}/suggestions/{suggestionId}`,
        `${host}|POST|/api/v1/holdings/acme/records/${module}/{id}/transition`,
      ])
      .join(','),
  };

  it('shows the wildcard module template also swallows the search paths', () => {
    for (const path of reserved)
      expect(nativeApiPath(wildcard, path, host, 'GET'), `${path} must expose the wildcard hazard`).toBe(true);
  });

  it('keeps all seven delegated families per exact module and excludes search', () => {
    for (const module of modules)
      for (const [suffix, method] of [
        ['', 'GET'],
        ['', 'POST'],
        ['/rec-1', 'GET'],
        ['/rec-1', 'PATCH'],
        ['/rec-1', 'DELETE'],
        ['/rec-1/references', 'POST'],
        ['/rec-1/mentions', 'POST'],
        ['/rec-1/suggestions', 'POST'],
        ['/rec-1/suggestions/sug-1', 'POST'],
        ['/rec-1/transition', 'POST'],
      ] as const)
        expect(
          nativeApiPath(exact, `/api/v1/holdings/acme/records/${module}${suffix}?section=overview`, host, method),
          `${method} /records/${module}${suffix}`,
        ).toBe(true);
    for (const path of reserved)
      expect(nativeApiPath(exact, path, host, 'GET'), `${path} must stay off the exact module template`).toBe(false);
    expect(nativeApiPath(exact, '/api/v1/holdings/acme/records/operations/rec-1/suggestions/sug-1/extra', host, 'POST')).toBe(false);
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

/** The three native-ready route families are gone from Next; the checked-in
 *  gateway config carries their Holding prefixes and the search rewrite. */
describe('Records/PMS/Stock Next route retirement and checked-in gateway entries', () => {
  const checkedIn = loadNativeApiPaths(resolve(root, 'scripts/local/native-api-paths.json'));
  const expanded = expandNativeApiPaths(checkedIn, holdingHosts);
  const gateway = {
    NODE_ENV: 'test' as const,
    ERPX_DEV_NATIVE_PATHS: expanded.paths,
    ERPX_DEV_NATIVE_REWRITES: expanded.rewrites ?? '',
  };
  const retired = [
    ...['recruit', 'launcher', 'learn', 'compat', 'sales', 'projects', 'admin', 'pms',
      'manufacturing', 'supply', 'pos', 'support', 'people', 'finance', 'service', 'legal', 'assets']
      .map(app => `apps/${app}/src/app/api/v1/holdings/[holdingSlug]/records/search/route.ts`),
    'apps/pms/src/app/api/v1/holdings/[holdingSlug]/pms/reports/route.ts',
    'apps/compat/src/app/api/v1/holdings/[holdingSlug]/pms/reports/route.ts',
    'apps/supply/src/app/api/v1/holdings/[holdingSlug]/stock-transfers/[[...path]]/route.ts',
    'apps/compat/src/app/api/v1/holdings/[holdingSlug]/stock-transfers/[[...path]]/route.ts',
  ];

  it('keeps the shared handlers and all 21 app re-exports unresolvable', () => {
    expect(retired).toHaveLength(21);
    for (const file of retired)
      expect(existsSync(resolve(root, file)), file).toBe(false);
    for (const file of [
      'packages/feature-pms/src/routes/api/v1/holdings/[holdingSlug]/pms/reports/route.ts',
      'packages/feature-supply/src/routes/api/v1/holdings/[holdingSlug]/stock-transfers/[[...path]]/route.ts',
    ]) expect(existsSync(resolve(root, file)), file).toBe(false);
    expect(existsSync(resolve(root, 'packages/server-records/src/adapters/record_search_http.rs'))).toBe(true);
  });

  it('matches every registered Holding to the native search, report and transfer prefixes', () => {
    for (const prefix of ['records/search', 'pms/reports', 'stock-transfers'])
      expect(checkedIn.holdingPrefixes, prefix).toContain(prefix);
    expect(checkedIn.rewrites).toContain('/records/search=/records/native-search');
    expect(expanded.rewrites).toBe('/records/search=/records/native-search');
    for (const { hostname, holdingSlug } of holdingHosts) {
      const search = `/api/v1/holdings/${holdingSlug}/records/search?q=%40user%2FAlice`;
      expect(nativeApiPath(gateway, search, hostname, 'GET'), hostname).toBe(true);
      expect(nativeApiRewrite(gateway, search)).toBe(`/api/v1/holdings/${holdingSlug}/records/native-search?q=%40user%2FAlice`);
      for (const [path, method] of [
        [`/api/v1/holdings/${holdingSlug}/pms/reports?view=range&property=property-1&from=2026-11-01&to=2026-11-02`, 'GET'],
        [`/api/v1/holdings/${holdingSlug}/pms/reports?view=entries&property=property-1&date=2026-11-01&page=2`, 'GET'],
        [`/api/v1/holdings/${holdingSlug}/pms/reports/range?property=property-1&from=2026-11-01&to=2026-11-02`, 'GET'],
        [`/api/v1/holdings/${holdingSlug}/stock-transfers`, 'POST'],
        [`/api/v1/holdings/${holdingSlug}/stock-transfers/transfer-1`, 'POST'],
        [`/api/v1/holdings/${holdingSlug}/stock-transfers/transfer-1/submit`, 'POST'],
      ] as const) {
        expect(nativeApiPath(gateway, path, hostname, method), `${hostname} ${method} ${path}`).toBe(true);
        expect(nativeApiPath(gateway, path, 'unregistered.example.test', method)).toBe(false);
      }
    }
  });

  it('pins owner SQL in each native handler and the {data} envelope in the browsers', () => {
    const records = read('packages/server-records/src/adapters/record_search.rs');
    expect(records).toContain('sqlx::query(AssertSqlSafe(');
    expect(records).toContain('.fetch_all(&mut *connection)');
    expect(records).toContain('SELECT module_id,id,title,status,version FROM workspace_records');
    const recordsHttp = read('packages/server-records/src/adapters/record_search_http.rs');
    expect(recordsHttp).toContain('/api/v1/holdings/{holding_slug}/records/native-search');
    expect(recordsHttp).toContain('backend.search(&host, &headers, &query)');
    const pms = read('packages/server-pms/src/adapters/report_http.rs');
    expect(pms).toContain('/api/v1/holdings/{holding_slug}/pms/reports/{report}');
    expect(pms).toContain('PmsReportKind::parse(view)');
    expect(pms).toContain('use sqlx::{PgConnection, Row, postgres::PgRow};');
    expect(pms).toContain('.fetch_all(&mut *c)');
    expect(pms).toContain('.fetch_one(&mut *c)');
    const transfersHttp = read('src/composition/src/inventory_owner_commands_http.rs');
    for (const path of [
      '/api/v1/holdings/{holding_slug}/stock-transfers',
      '/api/v1/holdings/{holding_slug}/stock-transfers/{id}',
      '/api/v1/holdings/{holding_slug}/stock-transfers/{id}/{action}',
    ]) expect(transfersHttp).toContain(path);
    expect(transfersHttp).toContain('post(post_transfer_transition::<C>)');
    const transfers = read('packages/server-inventory/src/adapters/transfers.rs');
    expect(transfers).toContain('UPDATE inventory_transfers SET');
    expect(transfers).toContain('INSERT INTO inventory_transfers(');
    expect(transfers).toContain('.execute(&mut *c)');
    expect(read('packages/feature-pms/src/components/range-reports.tsx')).toContain('return json.data;');
    expect(read('packages/feature-inventory/src/components/use-transfer-command.ts')).toContain('result.data');
  });
});
