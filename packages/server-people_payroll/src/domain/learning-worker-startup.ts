import {z} from 'zod';
import {directorySchema} from '@erpx/server-people_payroll/domain/people-directory';
import {lifecycleInput} from '@erpx/feature-people_payroll/lib/people-lifecycle';
const id=z.string().min(1).max(180),contextKey=z.string().regex(/^[a-f0-9]{64}$/),reason=z.string().trim().min(3).max(1000);
/** Bounded Learning-only extension to people.learner-reference: Company HR may
 * prepare/link one explicit existing Company account. Generic HR is unchanged. */
export const learningWorkerAccountsSchema=z.object({query:z.string().trim().max(120).optional()}).strict();
export const learningWorkerPeopleSchema=learningWorkerAccountsSchema;
export const learningWorkerSelectionSchema=z.union([z.object({}).strict(),z.object({accountId:id,personId:id.optional()}).strict(),z.object({personId:id}).strict()]);
export const learningWorkerProfileSchema=directorySchema.omit({linkedMemberUserId:true}).extend({workerType:z.literal('employee'),acceptRecommendedPay:z.literal(false).default(false),acceptRecommendedLeave:z.literal(false).default(false)}).strict();
export const learningWorkerPrepareSchema=z.discriminatedUnion('mode',[
 z.object({mode:z.literal('create'),accountId:id,expectedVersion:z.literal(0),contextKey,input:learningWorkerProfileSchema}).strict(),
 z.object({mode:z.literal('link'),accountId:id,personId:id,expectedVersion:z.number().int().positive(),expectedEngagementVersion:z.number().int().positive(),contextKey,reason}).strict(),
]);
export const learningWorkerPlacementSchema=lifecycleInput.pick({positionId:true,managerPersonId:true,effectiveAt:true,timezone:true,reason:true}).extend({positionId:id,expectedVersion:z.literal(0),contextKey}).strict();
export const learningWorkerPlacementSubmitSchema=learningWorkerPlacementSchema.extend({token:contextKey}).strict();
export const learningWorkerPlacementCancelSchema=z.object({expectedVersion:z.number().int().positive()}).strict();
export type LearningWorkerSelection=z.infer<typeof learningWorkerSelectionSchema>;
export type LearningWorkerPrepareInput=z.infer<typeof learningWorkerPrepareSchema>;
export type LearningWorkerPlacementInput=z.infer<typeof learningWorkerPlacementSchema>;
export type LearningWorkerPlacementSubmitInput=z.infer<typeof learningWorkerPlacementSubmitSchema>;
export type LearningWorkerPlacementCancelInput=z.infer<typeof learningWorkerPlacementCancelSchema>;
export type LearningWorkerStartupView={
 companyId:string;available:boolean;schemaAvailable:boolean;contextKey:string|null;
 account:{id:string;name:string;email:string}|null;
 person:{id:string;legalName:string;version:number}|null;
 engagement:{id:string;version:number;status:string;branchId:string|null;departmentId:string|null;positionId:string|null;levelId:string|null;managerPersonId:string|null;startDate:string;currency:string;jobTitle:string}|null;
 placement:{id:string;status:string;version:number;effectiveAt:number}|null;
 readiness:'selection-required'|'account-required'|'schema-required'|'person-required'|'link-required'|'placement-required'|'placement-pending'|'ready'|'ambiguous'|'unsupported-person'|'authority-required'|'purchase-required';
 canPrepare:boolean;canLink:boolean;canPreviewPlacement:boolean;canSubmitPlacement:boolean;canCancelPlacement:boolean;
 branches:{id:string;name:string}[];departments:{id:string;branchId:string|null;name:string}[];
 levels:{id:string;name:string}[];positions:{id:string;title:string;branchId:string|null;departmentId:string|null;levelId:string;isCeo:boolean}[];managers:{id:string;name:string}[];
};
export type LearningWorkerPrepareResult={id:string;version:number};
export type LearningWorkerPlacementResult={id:string;version:number;status:string};
export type LearningWorkerPlacementPreview={token:string;ready:boolean;blockers:string[]};
/** Original cancel does not serialize a version. */
export type LearningWorkerPlacementCancelResult={id:string;status:'cancelled'};

export type LearningWorkerStartupAccounts={companyId:string;accounts:{id:string;name:string;email:string}[]};
export type LearningWorkerProfileInput=z.infer<typeof learningWorkerProfileSchema>;
export type LearningWorkerAccountsInput=z.infer<typeof learningWorkerAccountsSchema>;
export type LearningWorkerPeopleInput=z.infer<typeof learningWorkerPeopleSchema>;
export type LearningWorkerStartupPeople={companyId:string;people:{id:string;legalName:string;version:number;engagementId:string;engagementVersion:number}[]};
