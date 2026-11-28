import { z } from "zod";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { BusinessRuleError } from "@erpx/platform-shared-server/index";
import type { SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  createBetaBootstrapEffects,
  type BetaHoldingRepository,
} from "@erpx/manage-erpx-server/application/onboarding";
import {
  onboardingSchema,
  requireVerified as verified,
  type BetaUser,
} from "@erpx/manage-erpx-server/domain/onboarding";
export { onboardingSchema } from "@erpx/manage-erpx-server/domain/onboarding";
export type { BetaUser } from "@erpx/manage-erpx-server/domain/onboarding";
import { betaOnboardingEffects as owner0 } from "@erpx/people-payroll-server/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner1 } from "@erpx/identity-access-server/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner2 } from "@erpx/organization-server/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner3 } from "@erpx/platform-runtime-server/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner4 } from "@erpx/billing-server/public/beta-onboarding-effects";
import { nativeOwnerWrite, NATIVE_OWNER_WRITE_NOOP_SQL } from "./native-owner-write";

/** Retained beta bootstrap effects for team invitations and seeded fixtures.
 * Company onboarding itself is owned by the native command router. */
function bindHolding(db: SqlDatabase): BetaHoldingRepository<SqlStatement> {
  return {
    // Native axum owner write owns this insert. The returned statement is an
    // inert placeholder; no write SQL remains in TypeScript.
    async prepareMembership(id: string, invitation: string | null, now: number) {
      return nativeOwnerWrite('me_beta_membership_insert',[id,invitation,now],`me-member:${id}`,{now},()=>db.prepare(NATIVE_OWNER_WRITE_NOOP_SQL)) as unknown as SqlStatement;
    },
    ...owner0(db),
    ...owner1(db),
    ...owner2(db),
    ...owner3(db),
    ...owner4(db),
  } as unknown as BetaHoldingRepository<SqlStatement>;
}
function exposedSync<T>(action: () => T) {
  try {
    return action();
  } catch (error) {
    if (error instanceof BusinessRuleError)
      throw new ApiError(error.status, error.code, error.message);
    throw error;
  }
}
export function requireVerified(user: BetaUser | undefined): asserts user is BetaUser {
  exposedSync(() => verified(user));
}
export function memberStatements(
  db: SqlDatabase,
  company: string,
  branch: string,
  user: BetaUser,
  owner: boolean,
  invitation: string | null,
  grant?: { parentId: string; validUntil: number },
) {
  return createBetaBootstrapEffects(bindHolding(db), { now: () => Date.now() }).memberStatements(
    company,
    branch,
    user,
    owner,
    invitation,
    grant,
  );
}
export function branchStatements(
  db: SqlDatabase,
  company: string,
  id: string,
  name: string,
  slug: string,
) {
  return createBetaBootstrapEffects(bindHolding(db), { now: () => Date.now() }).branchStatements(
    company,
    id,
    name,
    slug,
  );
}
