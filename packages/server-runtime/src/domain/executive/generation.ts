import type { Generation } from "@erpx/server-runtime/public/rules/canvas-domain";
export type Job = {
    id: string;
    report_id: string;
    company_id: string;
    user_id: string;
    assignment_id: string;
    scope_key: string;
    question: string;
    input_json: string;
    actor_json: string;
    provider_json: string;
    state: Generation["state"];
    proposal_json: string | null;
    error: string | null;
    created_at: number;
    lease_token: string;
};
