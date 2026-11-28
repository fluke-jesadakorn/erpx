import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { ManualRate } from '@erpx/finance-ui/lib/company-money';
import {
  companyMoneyConfig,
  companyMoneyGet,
  fail,
  record,
  validIdentifier,
} from './native-company-money';

/**
 * Native Company Money rate-list client. Carries no SQL and offers no
 * fallback: the authoritative SQL and quote normalization live in the Rust
 * owner. The `db` argument is retained only for signature compatibility.
 */
const validRate = (value: unknown): value is ManualRate =>
  record(value) &&
  typeof value.id === 'string' &&
  value.id.length > 0 &&
  typeof value.currency === 'string' &&
  value.currency.length > 0 &&
  typeof value.effectiveDate === 'string' &&
  typeof value.currencyPerUsd === 'string' &&
  typeof value.confirmedAt === 'number' &&
  Number.isSafeInteger(value.confirmedAt);

export async function companyRates(
  _db: SqlDatabase,
  companyId: string,
): Promise<ManualRate[]> {
  const settings = companyMoneyConfig();
  if (!validIdentifier(companyId)) return fail(422);
  const payload = await companyMoneyGet(settings, companyId, 'rates');
  if (!record(payload) || !Array.isArray(payload.data) || !payload.data.every(validRate))
    return fail(502);
  return payload.data as ManualRate[];
}
