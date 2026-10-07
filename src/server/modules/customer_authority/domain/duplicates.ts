import {z} from "zod";
export type DuplicateRow=Record<string,unknown>&{id:string;left_customer_id:string;right_customer_id:string;left_name:string;right_name:string;status:string;canonical_customer_id:string|null;created_by:string;version:number;updated_at:number;canReview:boolean;canReopen:boolean;canMerge:boolean};
const reason=z.string().trim().min(3).max(2000),version=z.number().int().positive();
export const duplicateChangeSchema=z.discriminatedUnion('action',[z.object({action:z.literal('distinct'),expectedVersion:version,reason}),z.object({action:z.literal('duplicate'),expectedVersion:version,canonicalCustomerId:z.string().min(1),reason}),z.object({action:z.literal('reopen'),expectedVersion:version,reason}),z.object({action:z.literal('merge'),expectedVersion:version,reason})]);

export type DuplicateChange = z.infer<typeof duplicateChangeSchema>;
