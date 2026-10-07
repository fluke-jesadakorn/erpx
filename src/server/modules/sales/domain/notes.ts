import {z} from 'zod';
import type {Row} from '../../commerce/public/commerce';
export type ContactNote={id:string;company_id:string;customer_id:string;order_id:string|null;channel:string;occurred_at:number;body:string;status:string;version:number;created_by:string;created_at:number;updated_at:number};
export type CustomerNotesView={customer:{id:string;name:string;code:string;version:number};order?:{id:string;party_id:string;party_name:string;number:string;version:number};notes:(ContactNote&{canChange:boolean})[];history:Row[];canCreate:boolean};

const body=z.string().trim().min(3).max(4000),version=z.number().int().positive();
export const customerNoteSchema=z.object({orderId:z.string().min(1).optional(),customerId:z.string().min(1).optional(),expectedVersion:version,channel:z.enum(['phone','email','meeting','other']),occurredAt:z.number().int().nonnegative(),body}).superRefine((value,ctx)=>{if(Boolean(value.orderId)===Boolean(value.customerId))ctx.addIssue({code:'custom',path:['source'],message:'Choose exactly one order or customer source.'});});
export const customerNoteChangeSchema=z.discriminatedUnion('action',[z.object({action:z.literal('correct'),expectedVersion:version,body,reason:body}),z.object({action:z.literal('archive'),expectedVersion:version,reason:body})]);

export type CustomerNoteCreate=z.infer<typeof customerNoteSchema>;
export type CustomerNoteChange=z.infer<typeof customerNoteChangeSchema>;
export function customerNoteInScope(note:Row,branchId:string|null,departmentId:string|null){return note.order_id===null?branchId===null&&departmentId===null:(branchId===null||note.branch_id===branchId)&&(departmentId===null||note.department_id===departmentId);}
