import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { nativeApiPath } from '../../../scripts/local/native-api-proxy';

const repo = resolve(__dirname, '../../..');
const read = (path: string) => readFileSync(resolve(repo, path), 'utf8');
const dir = 'packages/feature-service/src/routes/api/v1/holdings/[holdingSlug]/field-service';
const ROUTE = `${dir}/route.ts`;
const RETIRED = `${dir}/native-reserve.ts`;
const CLIENT = `${dir}/native-part-reservation.ts`;
const COMMAND = 'packages/feature-field_service/src/lib/native-reserve-command.ts';
const SERVICE_ENTRY = 'apps/service/src/app/api/v1/holdings/[holdingSlug]/field-service/route.ts';
const COMPAT_ENTRY = 'apps/compat/src/app/api/v1/holdings/[holdingSlug]/field-service/route.ts';

describe('field_service part-reservation native cutover', () => {
  test('the strict native client owns the exact mounted read and fails closed', () => {
    expect(existsSync(resolve(repo, CLIENT))).toBe(true);
    const client = read(CLIENT);
    expect(client).toContain('/parts-reservations/${encodeURIComponent(reservationId)}');
    expect(client).toContain('x-erpx-context-company');
    expect(client).toContain('ERPX_NATIVE_FIELD_SERVICE_RESERVE_CONFIG');
    expect(client).toContain("'consumedMilli','expiresAt','id','orderId','productId','quantityMilli','releasedMilli','warehouseId'");
    expect(client).toContain('503');
    expect(client).not.toContain('SELECT ');
  });

  test('the reservation readback is native-only after the shared route retirement', () => {
    expect(existsSync(resolve(repo, ROUTE))).toBe(false);
    expect(read(RETIRED)).toContain('nativeReservePost');
    expect(read(RETIRED)).not.toContain('nativeReserveReadback');
    expect(read(COMMAND)).toContain('export function createNativeReserveCommands');
    expect(read(COMMAND)).toContain('nativeReservationConfirmed');
  });

  test('the retired TypeScript reservation validator/readback is deleted', () => {
    const retired = read(RETIRED);
    expect(retired).not.toContain('nativeReserveReadback');
    expect(retired).not.toContain('FieldServiceReservationReadResponse');
    expect(retired).not.toContain('function readback(');
    expect(retired).toContain('retainedIssueEvidence');
  });

  test('the app re-exports no longer front a retired shared handler', () => {
    for (const entry of [SERVICE_ENTRY, COMPAT_ENTRY]) {
      expect(existsSync(resolve(repo, entry)), entry).toBe(false);
    }
  });
});

const host = 'acme.jesadakorn.com';
const base = '/api/v1/holdings/{holdingSlug}/field-service';
/** Exact host|method|path entries the lead persists in ERPX_DEV_NATIVE_PATHS.
 * No catch-all wildcard: every served subpath/method is an exact template. */
const gatewayEntries = [
  `${host}|GET|${base}/collaboration`,
  `${host}|POST|${base}/collaboration`,
  `${host}|GET|${base}/operations`,
  `${host}|POST|${base}/operations`,
  `${host}|GET|${base}/customer-preparation`,
  `${host}|POST|${base}/customer-preparation`,
  `${host}|POST|${base}/customer-preparation/confirmations`,
  `${host}|GET|${base}/initial-physical-stock`,
  `${host}|POST|${base}/initial-physical-stock`,
  `${host}|GET|${base}/initial-physical-stock/{id}`,
  `${host}|GET|${base}/initial-physical-stock/{id}/flow`,
  `${host}|POST|${base}/initial-physical-stock/{id}/flow`,
  `${host}|POST|${base}/initial-physical-stock/{id}/{action}`,
  `${host}|GET|${base}/parts-setup`,
  `${host}|POST|${base}/parts-setup`,
  `${host}|GET|${base}/parts-setup/prerequisites`,
  `${host}|GET|${base}/parts-setup/stock`,
  `${host}|POST|${base}/parts-setup/existing`,
  `${host}|POST|${base}/parts-setup/masters/products`,
  `${host}|POST|${base}/parts-setup/masters/warehouses`,
  `${host}|POST|${base}/parts-setup/charts`,
  `${host}|POST|${base}/parts-setup/accounts`,
  `${host}|POST|${base}/parts-setup/periods`,
  `${host}|POST|${base}/parts-setup/rates`,
  `${host}|GET|${base}`,
  `${host}|POST|${base}`,
] as const;

const retiredHandlers = [
  `${dir}/route.ts`,
  `${dir}/collaboration/route.ts`,
  `${dir}/operations/route.ts`,
  `${dir}/customer-preparation/[[...path]]/route.ts`,
  `${dir}/initial-physical-stock/[[...path]]/route.ts`,
  `${dir}/parts-setup/[[...path]]/route.ts`,
] as const;
const retiredReexports = [
  ...['service', 'compat'].map(app =>
    `apps/${app}/src/app/api/v1/holdings/[holdingSlug]/field-service/route.ts`),
  'apps/service/src/app/api/v1/holdings/[holdingSlug]/field-service/collaboration/route.ts',
  'apps/service/src/app/api/v1/holdings/[holdingSlug]/field-service/operations/route.ts',
  'apps/service/src/app/api/v1/holdings/[holdingSlug]/field-service/customer-preparation/[[...path]]/route.ts',
  'apps/service/src/app/api/v1/holdings/[holdingSlug]/field-service/initial-physical-stock/[[...path]]/route.ts',
  'apps/service/src/app/api/v1/holdings/[holdingSlug]/field-service/parts-setup/[[...path]]/route.ts',
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/field-service/customer-preparation/[[...path]]/route.ts',
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/field-service/initial-physical-stock/[[...path]]/route.ts',
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/field-service/parts-setup/[[...path]]/route.ts',
] as const;

describe('field service Next API retirement', () => {
  test('retired handlers and every app re-export no longer exist', () => {
    for (const path of [...retiredHandlers, ...retiredReexports])
      expect(existsSync(resolve(repo, path)), path).toBe(false);
  });

  test('exact gateway entries match nativeApiPath and do not over-match', () => {
    const environment = { NODE_ENV: 'test' as const, ERPX_DEV_NATIVE_PATHS: gatewayEntries.join(',') };
    const concrete = (template: string) => template
      .replace('{holdingSlug}', 'acme')
      .replace('{id}', 'count-a')
      .replace('{action}', 'approve');
    for (const entry of gatewayEntries) {
      const [entryHost, method, template] = entry.split('|');
      const path = concrete(template!);
      expect(nativeApiPath(environment, path, entryHost, method), entry).toBe(true);
      expect(nativeApiPath(environment, path, 'other.example.test', method), entry).toBe(false);
    }
    expect(nativeApiPath(environment, '/api/v1/holdings/acme/field-service/collaboration', host, 'PUT')).toBe(false);
    expect(nativeApiPath(environment, '/api/v1/holdings/acme/field-service/collaboration/extra', host, 'GET')).toBe(false);
    expect(nativeApiPath(environment, '/api/v1/holdings/acme/field-service/parts-setup/charts', host, 'GET')).toBe(false);
    expect(nativeApiPath(environment, '/api/v1/holdings/acme/field-service/parts-setup/masters/products/extra', host, 'POST')).toBe(false);
    expect(nativeApiPath(environment, '/api/v1/holdings/acme/field-service/unknown', host, 'GET')).toBe(false);
  });

  test('the native Axum owners register every retired path and execute SQL', () => {
    const router = read('src/composition/src/field_service_reservation_read_http.rs');
    for (const route of [
      '/field-service/collaboration', '/field-service/operations',
      '/field-service/customer-preparation', '/field-service/customer-preparation/confirmations',
      '/field-service/initial-physical-stock', '/field-service/initial-physical-stock/{id}',
      '/field-service/initial-physical-stock/{id}/flow', '/field-service/initial-physical-stock/{id}/{action}',
      '/field-service/parts-setup', '/field-service/parts-setup/prerequisites',
      '/field-service/parts-setup/stock', '/field-service/parts-setup/existing',
      '/field-service/parts-setup/masters/products', '/field-service/parts-setup/masters/warehouses',
      '/field-service/parts-setup/charts', '/field-service/parts-setup/accounts',
      '/field-service/parts-setup/periods', '/field-service/parts-setup/rates',
      '/field-service/workspace', '/field-service/orders/{id}/detail',
      '/field-service/requests/{key}', '/field-service/orders/{key}', '/field-service/visits/{key}',
      '/field-service/agreements/reviews/{key}', '/field-service/agreements/creates/{key}',
      '/field-service/billings/{key}', '/field-service/{order_id}/issues/{key}',
    ]) expect(router, route).toContain(route);
    expect(read('src/composition/src/field_service_http_core.rs')).toContain('"/api/v1/holdings/{holding_slug}/field-service"');
    expect(read('packages/server-field_service/src/adapters/reservation_part_http.rs')).toContain('/parts-reservations/{reservation_id}');
    const service = read('src/composition/src/field_service_reservation_read.rs');
    expect(service).toContain('sqlx::query_scalar');
    expect(service).toContain('execute_with_sources_deferred');
  });
});
