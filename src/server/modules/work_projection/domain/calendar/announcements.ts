export type AnnouncementRow = {
    id: string;
    company_id: string;
    branch_id: string | null;
    department_id: string | null;
    title: string;
    content: string;
    start_date: string;
    end_date: string;
    status: string;
    version: number;
    created_by: string;
    created_at: number;
    updated_at: number;
};
