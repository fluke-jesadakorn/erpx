import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import contract from './package-obligation-plan-sources.json' with { type: 'json' };

/** Allow-listed native Legal package-obligation read kind. */
export const LEGAL_PACKAGE_OBLIGATION_PLAN = 'legal_package_obligations';

const MAX_SQL = 200_000;
const PREFIX = 'WITH scope AS(';
const SUFFIX = 'ORDER BY kind,id,fact';

export type LegalPackageObligationPlan = Readonly<{
  sql: string;
  values: readonly [string, number];
}>;

/**
 * Native-owned plan for the company-wide retained Legal obligations. The
 * statement lives in the owner's JSON contract and is executed on the caller's
 * already-open unit, so the read observes the caller's uncommitted grants,
 * holds, shares and feature rows while the source locks stay held. No SQL
 * remains in the caller and there is no TypeScript fallback.
 */
export async function nativeLegalPackageObligationPlan(
  companyId: string,
  now: number,
): Promise<LegalPackageObligationPlan> {
  if (!companyId || companyId.length > 200 || !Number.isSafeInteger(now) || now < 0)
    throw new ApiError(
      422,
      'LEGAL_PACKAGE_OBLIGATION_SCOPE_REQUIRED',
      'Use one bounded Legal package obligation scope.',
    );
  const sql = contract.sql;
  if (
    typeof sql !== 'string' ||
    sql.length === 0 ||
    sql.length > MAX_SQL ||
    !sql.startsWith(PREFIX) ||
    !sql.endsWith(SUFFIX) ||
    !sql.includes('FROM legal_items i,scope') ||
    !sql.includes("'retention-policy'")
  )
    throw new ApiError(
      502,
      'NATIVE_LEGAL_PACKAGE_READ_RESPONSE_INVALID',
      'The native Legal package read was invalid.',
    );
  return { sql, values: [companyId, now] };
}
