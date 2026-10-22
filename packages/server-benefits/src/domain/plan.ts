import {z} from "zod";
const date=z.string().regex(/^\d{4}-\d{2}-\d{2}$/),currency=z.string().length(3).transform((x)=>x.toUpperCase());
export const benefitPlanSchema=z.object({
  name: z.string().trim().min(2).max(160),
  provider: z.string().trim().min(2).max(160),
  coverageType: z.enum(["employee", "family"]),
  currency,
  employeeDeduction: z.string().regex(/^\d+(\.\d{1,2})?$/),
  employerCost: z.string().regex(/^\d+(\.\d{1,2})?$/),
  effectiveFrom: date,
  effectiveTo: date.nullable().optional(),
  requiredEligibilityCode:z.string().trim().min(2).max(120).nullable().optional(),
});
export type BenefitPlanInput=z.infer<typeof benefitPlanSchema>;
