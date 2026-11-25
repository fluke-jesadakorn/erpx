import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeLegalReadPlan } from './native-legal-auxiliary';

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
 * Native owner plan for the company-wide retained Legal obligations. The
 * caller executes the statement on its already-open unit so the read observes
 * the caller's uncommitted grants, holds, shares and feature rows while the
 * source locks stay held. No SQL remains in the caller and there is no
 * TypeScript fallback.
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
  const plan = await nativeLegalReadPlan(LEGAL_PACKAGE_OBLIGATION_PLAN, [companyId, now]);
  if (
    plan.sql.length > MAX_SQL ||
    !plan.sql.startsWith(PREFIX) ||
    !plan.sql.endsWith(SUFFIX) ||
    !plan.sql.includes("FROM legal_items i,scope") ||
    !plan.sql.includes("'retention-policy'") ||
    plan.values.length !== 2 ||
    plan.values[0] !== companyId ||
    plan.values[1] !== now
  )
    throw new ApiError(
      502,
      'NATIVE_LEGAL_PACKAGE_READ_RESPONSE_INVALID',
      'The native Legal package read was invalid.',
    );
  return { sql: plan.sql, values: [companyId, now] };
}
