import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { SqlResult } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { Row } from '@erpx/server-commerce/public/commerce';
import {
  nativeCustomerRead,
  FINANCE_LANE5_RD_PRICING_CUSTOMERS,
  FINANCE_LANE5_RD_PRICING_CUSTOMER_VALID,
  FINANCE_LANE5_RD_CUSTOMER_PROFILE,
  FINANCE_LANE5_RD_CUSTOMER_DIRECTORY,
  FINANCE_LANE5_RD_NOTE_CUSTOMER,
} from './native-lane-5/client';

/**
 * Finance-owned customer master reads. Every statement, projection and page
 * cursor now lives in the native `native_lane_5` axum/sqlx owner; these
 * functions only transport bounded identities and validate the native result.
 * There is no TypeScript SQL fallback.
 */

export function salesPricingCustomers(write: SqlDatabase, actor: WorkContext): Promise<SqlResult<Row>> {
  void write;
  return nativeCustomerRead<Row[]>(FINANCE_LANE5_RD_PRICING_CUSTOMERS, actor).then((results) => ({
    results,
    success: true,
    meta: { changes: 0, duration: 0 },
  }));
}

export async function salesPricingCustomerValid(write: SqlDatabase, actor: WorkContext, id: string, currency: string): Promise<boolean> {
  void write;
  return nativeCustomerRead<boolean>(FINANCE_LANE5_RD_PRICING_CUSTOMER_VALID, actor, { id, currency });
}

export async function salesCustomerProfile(write: SqlDatabase, actor: WorkContext, id: string): Promise<Row | null> {
  void write;
  return nativeCustomerRead<Row | null>(FINANCE_LANE5_RD_CUSTOMER_PROFILE, actor, { id });
}

export async function salesCustomerDirectory(
  write: SqlDatabase,
  actor: WorkContext,
  search: string,
  status: string,
  cursor?: string,
): Promise<{ items: Row[]; nextCursor: string | null; previousCursor: string | null; hasMore: boolean }> {
  void write;
  return nativeCustomerRead<{
    items: Row[];
    nextCursor: string | null;
    previousCursor: string | null;
    hasMore: boolean;
  }>(FINANCE_LANE5_RD_CUSTOMER_DIRECTORY, actor, { search, status, cursor });
}

export function salesNoteCustomer(
  write: SqlDatabase,
  actor: WorkContext,
  id: string,
  version?: number,
): Promise<{ id: string; name: string; code: string; version: number } | null> {
  void write;
  return nativeCustomerRead<{ id: string; name: string; code: string; version: number } | null>(
    FINANCE_LANE5_RD_NOTE_CUSTOMER,
    actor,
    { id, version: version ?? null },
  );
}
