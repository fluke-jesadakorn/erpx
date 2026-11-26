import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

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

  test('only the reservation branch is switched; the other readbacks stay retained', () => {
    const route = read(ROUTE);
    expect(route).toContain('nativePartReservationReadback(r,slug,order,reservation)');
    expect(route).not.toContain('nativeReserveReadback(');
    for (const retained of ['nativeIssueReadback', 'nativeRequestReadback', 'nativeOrderCreateReadback', 'nativeVisitReadback', 'nativeBillingReadback', 'nativeAgreementReviewReadback']) {
      expect(route, retained).toContain(retained);
    }
    expect(read(RETIRED)).toContain('nativeReservePost');
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

  test('the app re-exports still front the shared handler after the switch', () => {
    for (const entry of [SERVICE_ENTRY, COMPAT_ENTRY]) {
      expect(read(entry)).toContain('export { GET, POST } from "@erpx/feature-service/routes/api/v1/holdings/[holdingSlug]/field-service/route"');
    }
  });
});
