export type Person = { id: string; name: string; branchId: string | null };
export type PostPurpose = "update" | "plan" | "blocker" | "handover" | "announcement";
export type AudienceOption = {
  value: string;
  kind: "company" | "branch" | "department" | "participants" | "team" | "project";
  label: string;
};
export type Draft = {
  id: string;
  body: string;
  branch_id: string | null;
  department_id?: string | null;
  audience_kind?: AudienceOption["kind"] | null;
  audience_id?: string | null;
  participants_json?: string;
  sources_json?: string;
  purpose?: PostPurpose;
  version: number;
  mentions_json?: string;
  content_json?: string | null;
  rich_content_json?: string | null;
};
export type Meta = {
  handoffContext?: { companyId: string; userId: string; assignmentId: string; base: string };
  people: Person[];
  branches: { id: string; display_name: string }[];
  audiences?: AudienceOption[];
  projects?: { id: string; title: string }[];
  drafts: Draft[];
  tasks: {
    id: string;
    title: string;
    state: string;
    assignee_user_id: string;
    due_at: number;
  }[];
};
