import {z} from "zod";
import {BusinessRuleError} from "../../shared";
export const positionPayInput = z.object({
  payGroupId: z.string().min(1), currency: z.string().length(3),
  monthlyAmount: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER), expectedVersion: z.number().int().min(0),
});
export const levelLeaveInput = z.object({
  annualDays: z.number().min(0).max(365), sickDays: z.number().min(0).max(365),
  expectedVersion: z.number().int().min(0),
});


export type PositionPayDefault=z.infer<typeof positionPayInput>;export type LevelLeaveDefault=z.infer<typeof levelLeaveInput>;
export type DefaultRecommendation={engagementId:string;positionId:string|null;levelId:string|null;priorLevelId:string|null;startDate:string;currency:string;acceptPay:boolean;acceptLeave:boolean;commandId:string};
export type PayDefaultFact={pay_group_id:string;currency:string;monthly_amount_minor:number;version:number};
export type LeaveDefaultFact={policy_version_id:string;effective_from:string};
export type ExistingLeaveFact={id:string;effective_from:string;effective_to:string|null;policy_key:string};
export function assertCompensationAccess(canRead:boolean){if(!canRead)throw new BusinessRuleError(403,'PAY_ACCESS_REQUIRED','Compensation access is required.');}
export function assertDefaultVersion(current:number,expected:number,label:'position'|'level'){if(current!==expected)throw new BusinessRuleError(409,'DEFAULT_CHANGED',`Refresh the ${label} default and try again.`);}
export function leaveDefaultRecommendation(options:DefaultRecommendation,policy:LeaveDefaultFact|null,existing:ExistingLeaveFact|null){
 if(!policy)return null;const managed=!existing||existing.policy_key==='legacy-untracked'||/^level-[1-6]-default$/.test(existing.policy_key);if(!managed||existing&&options.priorLevelId===options.levelId&&existing.policy_key!=='legacy-untracked')return null;
 const changed=Boolean(options.priorLevelId&&options.priorLevelId!==options.levelId),year=Number(options.startDate.slice(0,4)),effective=changed?`${year+1}-01-01`:options.startDate<policy.effective_from?policy.effective_from:options.startDate;
 if(existing&&existing.effective_from>=effective)throw new BusinessRuleError(409,'LEAVE_ASSIGNMENT_OVERLAP','Review the current leave assignment before accepting this default.');const end=changed?`${year}-12-31`:new Date(Date.parse(`${effective}T00:00:00Z`)-86400000).toISOString().slice(0,10);
 return {policyVersionId:policy.policy_version_id,effective,end,endAssignmentId:existing&&existing.effective_from<=end&&(!existing.effective_to||existing.effective_to>end)?existing.id:null};
}
