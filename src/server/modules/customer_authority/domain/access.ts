import {z} from "zod";
export type AccessRow=Record<string,unknown>&{id:string;number:string;email:string;status:string;expires_at:number;version:number;created_by:string;approved_by:string|null;reviewed_at:number|null;order_id:string;branch_id:string;department_id:string|null;updated_at:number;reviewers?:number;canReview:boolean;canRevoke:boolean};
const reason=z.string().trim().min(3).max(2000),version=z.number().int().positive();
export const customerAccessRequestSchema=z.object({orderId:z.string().min(1),sourceVersion:version,email:z.email().max(254).transform(s=>s.toLowerCase()),expiresAt:z.number().int().positive(),reason});
export const customerAccessChangeSchema=z.object({action:z.enum(['approve','reject','revoke']),expectedVersion:version,reason});

export type AccessRequest = z.infer<typeof customerAccessRequestSchema>;
export type AccessChange = z.infer<typeof customerAccessChangeSchema>;
