import {z} from "zod";
const reason=z.string().trim().min(3).max(2000);
export const requestSchema=z.object({leftCustomerId:z.string().min(1),rightCustomerId:z.string().min(1),evidence:reason}).refine(x=>x.leftCustomerId!==x.rightCustomerId,{message:'Compare two different customer records.'});
export type DuplicateRequestInput=z.infer<typeof requestSchema>;
