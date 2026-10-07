import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { HoldingDirectoryNode } from "@/server/modules/organization/public/holding-people-directory";
export type CompanyRow = {
    id: string;
    slug: string;
    display_name: string;
};
export type NodeRow = CompanyRow & {
    company_id: string;
    kind: HoldingDirectoryNode["kind"];
    parent_id: string | null;
    branch_id: string | null;
};
export type LevelRow = {
    id: string;
    company_id: string;
    code: string;
    name: string;
    level_number: number | null;
};
export type PositionRow = {
    id: string;
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    title: string;
    level_id: string;
    reports_to_position_id: string | null;
    is_ceo: number;
};
export type PersonRow = {
    person_id: string;
    user_id: string | null;
    engagement_id: string;
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    position_id: string | null;
    legal_name: string;
    job_title: string;
    work_email: string | null;
    work_phone: string | null;
};
export type DirectorySelection = {
    kind: "company" | "branch" | "department" | "team" | "position" | "person" | "level" | "unassigned";
    id: string;
};
export type DirectorySelectionRecord = {
    companyId: string;
    person?: {
        id: string;
        personId: string;
        engagementId: string;
        companyId: string;
    };
};
export const archivedScope = (actor: WorkContext) => [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
export type SelectedPersonRow = {
    person_id: string;
    engagement_id: string;
    company_id: string;
};
