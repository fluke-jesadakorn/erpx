import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
export type PostRow = {
    id: string;
    kind: "team" | "record" | "system";
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    author_id: string;
    author_name: string;
    body: string;
    mentions_json: string;
    content_json: string | null;
    rich_content_json: string | null;
    audience_kind: "company" | "branch" | "department" | "participants" | "team" | "project" | null;
    audience_id: string | null;
    purpose: "update" | "plan" | "blocker" | "handover" | "announcement";
    edited_at: number | null;
    retracted_at: number | null;
    version: number;
    state: string;
    linked_module: string | null;
    created_at: number;
};
export function socialScopeKind(actor: Pick<WorkContext, 'departmentId'>) { return actor.departmentId ? 'department' : 'branch'; }
