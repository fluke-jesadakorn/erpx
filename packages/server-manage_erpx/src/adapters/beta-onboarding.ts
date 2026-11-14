import { z } from "zod";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { BusinessRuleError } from "@erpx/server-shared/index";
import type { SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {
  createBetaBootstrapEffects,
  type BetaHoldingRepository,
} from "@erpx/server-manage_erpx/application/onboarding";
import {
  onboardingSchema,
  requireVerified as verified,
  type BetaUser,
} from "@erpx/server-manage_erpx/domain/onboarding";
export { onboardingSchema } from "@erpx/server-manage_erpx/domain/onboarding";
export type { BetaUser } from "@erpx/server-manage_erpx/domain/onboarding";
import { betaOnboardingEffects as owner0 } from "@erpx/server-people_payroll/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner1 } from "@erpx/server-identity_access/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner2 } from "@erpx/server-organization/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner3 } from "@erpx/server-runtime/public/beta-onboarding-effects";
import { betaOnboardingEffects as owner4 } from "@erpx/server-billing/public/beta-onboarding-effects";

/** Retained beta bootstrap effects for team invitations and seeded fixtures.
 * Company onboarding itself is owned by the native command router. */
function bindHolding(db: SqlDatabase): BetaHoldingRepository<SqlStatement> {
  return {
    prepareMembership(id: string, invitation: string | null, now: number) {
      return db
        .prepare(
          "INSERT INTO beta_members(user_id,invitation_id,joined_at) VALUES(?,?,?) ON CONFLICT DO NOTHING",
        )
        .bind(id, invitation, now);
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
