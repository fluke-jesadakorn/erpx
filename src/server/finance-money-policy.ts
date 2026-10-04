import "server-only";
import { currencyScale } from "@/lib/company-money";
import { ApiError } from "./http";
import type { SqlDatabase } from "./runtime/types";

/** Existing Finance ledger amounts retain their original two-decimal meaning. */
export async function requireFinanceMoneyScale(db: SqlDatabase, companyId: string, currency: string) {
  const configured = await db.prepare("SELECT scale FROM company_currency_scales WHERE company_id=? AND currency=?").bind(companyId, currency).first<{ scale: number }>();
  let scale: number;
  try { scale = currencyScale(currency, configured ? Number(configured.scale) : undefined); }
  catch (error) { throw new ApiError(422, "CURRENCY_SCALE_REQUIRED", error instanceof Error ? error.message : "Configure the currency's minor units."); }
  if (scale !== 2) throw new ApiError(422, "FINANCE_CURRENCY_SCALE_UNSUPPORTED", "This Finance ledger currently accepts currencies with two minor-unit decimals. Historical amounts retain their recorded scale; use a supported currency for a new document.");
  return scale;
}
