import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { z } from 'zod';
import { getInvoice } from '../adapters/finance-invoice-read';
import { financeAgingBalancesNative, type AgingRow } from '../adapters/native-lane-2/client';

export { getInvoice };

/**
 * Native catch-all aging read. The Rust owner validates the filters, executes
 * the historical SQL and returns the complete row (including the decimal
 * presentation fields); this module only carries the WorkContext and rejects
 * an invalid cutoff with the legacy contract code.
 */
export async function nativeFinanceAging(
  actor: WorkContext,
  kind: 'payable' | 'receivable',
  asOf: string,
  chartId?: string,
): Promise<AgingRow[]> {
  if (!z.string().date().safeParse(asOf).success)
    throw new ApiError(422, 'INVALID_AS_OF', 'Use a valid accounting cutoff date.');
  return financeAgingBalancesNative(actor, asOf, kind, chartId);
}
