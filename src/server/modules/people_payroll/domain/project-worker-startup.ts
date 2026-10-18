import {z} from 'zod';
import {directorySchema} from './people-directory';
import {lifecycleInput} from '@/lib/people_payroll/people-lifecycle';
const id=z.string().min(1).max(180),contextKey=z.string().regex(/^[a-f0-9]{64}$/),reason=z.string().trim().min(3).max(1000);
/** Proposed bounded extension to people.worker-reference: Company HR may
 * prepare/link one explicit existing Company account. Generic HR is unchanged. */
export const projectWorkerAccountsSchema=z.object({query:z.string().trim().max(120).optional()}).strict();
export const projectWorkerPeopleSchema=projectWorkerAccountsSchema;
export const projectWorkerSelectionSchema=z.union([z.object({}).strict(),z.object({accountId:id,personId:id.optional()}).strict(),z.object({personId:id}).strict()]);
export const projectWorkerProfileSchema=directorySchema.omit({linkedMemberUserId:true}).extend({workerType:z.literal('employee'),acceptRecommendedPay:z.literal(false).default(false),acceptRecommendedLeave:z.literal(false).default(false)}).strict();
export const projectWorkerPrepareSchema=z.discriminatedUnion('mode',[
 z.object({mode:z.literal('create'),accountId:id,expectedVersion:z.literal(0),contextKey,input:projectWorkerProfileSchema}).strict(),
 z.object({mode:z.literal('link'),accountId:id,personId:id,expectedVersion:z.number().int().positive(),expectedEngagementVersion:z.number().int().positive(),contextKey,reason}).strict(),
]);
export const projectWorkerPlacementSchema=lifecycleInput.pick({positionId:true,managerPersonId:true,effectiveAt:true,timezone:true,reason:true}).extend({positionId:id,expectedVersion:z.literal(0),contextKey}).strict();
export const projectWorkerPlacementSubmitSchema=projectWorkerPlacementSchema.extend({token:contextKey}).strict();
export const projectWorkerPlacementCancelSchema=z.object({expectedVersion:z.number().int().positive()}).strict();
export type ProjectWorkerSelection=z.infer<typeof projectWorkerSelectionSchema>;
export type ProjectWorkerPrepareInput=z.infer<typeof projectWorkerPrepareSchema>;
export type ProjectWorkerPlacementInput=z.infer<typeof projectWorkerPlacementSchema>;
export type ProjectWorkerPlacementSubmitInput=z.infer<typeof projectWorkerPlacementSubmitSchema>;
export type ProjectWorkerPlacementCancelInput=z.infer<typeof projectWorkerPlacementCancelSchema>;
export type ProjectWorkerStartupView={
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
export type ProjectWorkerPrepareResult={id:string;version:number};
export type ProjectWorkerPlacementResult={id:string;version:number;status:string};
export type ProjectWorkerPlacementPreview={token:string;ready:boolean;blockers:string[]};
/** Original cancel does not serialize a version. */
export type ProjectWorkerPlacementCancelResult={id:string;status:'cancelled'};

export type ProjectWorkerStartupAccounts={companyId:string;accounts:{id:string;name:string;email:string}[]};
export type ProjectWorkerProfileInput=z.infer<typeof projectWorkerProfileSchema>;
export type ProjectWorkerAccountsInput=z.infer<typeof projectWorkerAccountsSchema>;
export type ProjectWorkerPeopleInput=z.infer<typeof projectWorkerPeopleSchema>;
export type ProjectWorkerStartupPeople={companyId:string;people:{id:string;legalName:string;version:number;engagementId:string;engagementVersion:number}[]};
