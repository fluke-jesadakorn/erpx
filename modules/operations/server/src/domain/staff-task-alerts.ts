export type StaffTaskAlertResult = {
    dueSoon: number;
    overdue: number;
};
export type TaskAlertRow = {
    project_id?:string|null;
    id: string;
    company_id: string;
    title: string;
    due_at: number | null;
    version: number;
    assignee_user_id: string;
    creator_user_id: string;
    project_owner_id: string | null;
    delegate_user_id: string | null;
};
