import 'server-only';
import { createRequireFinanceMoneyScale } from '@erpx/server-finance/application/money-policy';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {
  companyMoneyConfig,
  companyMoneyGet,
  fail,
  record,
  validCurrency,
} from './native-company-money';

/**
 * Native Company Money currency-scale client. Carries no SQL and offers no
 * fallback. ISO validation and the two-decimal ledger rule remain in the
 * shared TS application port; the native owner only reports configured scale.
 * The `db` argument is retained only for signature compatibility.
 */
export async function requireFinanceMoneyScale(
  _db: SqlDatabase,
  companyId: string,
  currency: string,
) {
  return createRequireFinanceMoneyScale({
    error: (...args) => new ApiError(...args),
    scale: async (companyId, currency) => {
      const settings = companyMoneyConfig();
      if (!validCurrency(currency)) return fail(422);
      const payload = await companyMoneyGet(
        settings,
        companyId,
        `scale?currency=${encodeURIComponent(currency)}`,
      );
      if (
        !record(payload) ||
        (payload.data !== null &&
          (typeof payload.data !== 'number' || !Number.isSafeInteger(payload.data)))
      )
        return fail(502);
      return payload.data as number | null;
    },
  })(companyId, currency);
}
