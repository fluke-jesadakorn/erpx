import { z } from 'zod';
export type Reply = {
    id: string;
    post_id: string;
    owner_type: "post" | "comment";
    owner_id: string;
    company_id: string;
    actor_id: string;
    actor_json: string;
    state: string;
    attempt: number;
    lease_token: string;
};
export const replySchema = z.object({ body: z.string().trim().min(1).max(10000) }).strict();
