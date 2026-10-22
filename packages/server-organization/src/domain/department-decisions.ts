import {z} from 'zod';
export const departmentOverrideInput=z.object({kind:z.enum(['expense','leave','overtime','request']),id:z.string().min(1).max(180),expectedVersion:z.number().int().positive(),outcome:z.enum(['approve','reject']),reason:z.string().trim().min(3).max(2048)});
export type DepartmentOverrideInput=z.infer<typeof departmentOverrideInput>;export type DepartmentOverrideKind=DepartmentOverrideInput['kind'];export type OvertimeOverrideAction='authorize'|'reject'|'return'|'verify';
export type SavedRequestApproval={currentStep?:number;route?:{steps?:{userId:string}[]};decisions?:{override?:boolean;userId:string;originalApproverUserId?:string;at:number;note?:string}[]};
export const savedRequestApproval=(value:string)=>JSON.parse(value) as SavedRequestApproval;
export function overtimeOverrideAction(outcome:'approve'|'reject',state:string|null,operation:string|null):OvertimeOverrideAction{return operation?.startsWith('overtime.')?operation.slice('overtime.'.length) as OvertimeOverrideAction:outcome==='reject'?state==='pending_authorization'?'reject':'return':state==='pending_authorization'?'authorize':'verify';}
