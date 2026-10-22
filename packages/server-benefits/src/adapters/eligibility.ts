import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { BenefitLearningRequirement } from "@erpx/server-benefits/domain/eligibility";

/** Read through the caller's transaction so handoff guards observe the same snapshot. */
export async function benefitLearningRequirement(db: SqlDatabase, companyId: string, planId: string): Promise<BenefitLearningRequirement | null> {
  const row = await db.prepare("SELECT required_eligibility_code FROM benefit_plans WHERE id=? AND company_id=?").bind(planId, companyId).first<{ required_eligibility_code: string | null }>();
  return row ? { requiredEligibilityCode: row.required_eligibility_code } : null;
}
