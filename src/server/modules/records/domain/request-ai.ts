import { z } from "zod";
import { serviceCategories } from "@/lib/employee-services";
export const requestAiInput = z.object({
    expectedVersion: z.number().int().positive(),
    retry: z.boolean().default(false),
});
export const requestAiResult = z.object({
    title: z.string().max(200).nullable(),
    description: z.string().max(10000).nullable(),
    category: z.enum(serviceCategories.map(([id]) => id)).nullable(),
    summary: z.string().max(2000),
    sources: z.array(z.literal("request")).min(1).max(1),
});
export type RequestAiResult = z.infer<typeof requestAiResult>;
export type Proposal = {
    id: string;
    company_id: string;
    request_id: string;
    source_version: number;
    provider_json: string;
    actor_json: string;
    state: string;
    result_json: string | null;
    error: string | null;
    attempts: number;
    lease_token: string | null;
    claim_until: number | null;
};
