import {z} from "zod";
import {sourceRecordKind} from "../../shared";
export const policySchema=z.object({area:z.string().trim().regex(/^[a-z][a-z0-9_-]{1,39}$/),name:z.string().trim().min(3).max(160),sourceKinds:z.array(sourceRecordKind).min(1).max(7),targetMinutes:z.number().int().min(1).max(10080)});
export type QueuePolicyInput=z.infer<typeof policySchema>;
