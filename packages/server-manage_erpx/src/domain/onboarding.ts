import {z} from 'zod';
import {BusinessRuleError} from '@erpx/server-shared/index';
export const onboardingSchema = z.object({
  companyName: z.string().trim().min(2).max(100),
  branchName: z.string().trim().min(2).max(100),
  slug: z.string().regex(/^[a-z][a-z0-9-]{2,39}$/),
});
export type BetaUser = {
  id: string;
  name: string;
  email: string;
  emailVerified: boolean;
};
export function requireVerified(
  user: BetaUser | undefined,
): asserts user is BetaUser {
  if (!user) throw new BusinessRuleError(401, "UNAUTHENTICATED", "Sign in to continue.");
  if (!user.emailVerified)
    throw new BusinessRuleError(
      403,
      "VERIFY_EMAIL",
      "Verify your email before joining or creating a company.",
    );
}
export type SlotRow = {
  holding_id: string;
  slug: string;
  company_name: string;
  branch_name: string;
  state: string;
};

export type BetaResourceSlot={id:string;write:string;read:string;files:string;beta?:boolean};
export type BetaCapacity={members:number;branches:number;ai:number;storage:number};
