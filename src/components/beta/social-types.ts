export type Person = { id: string; name: string; branchId: string | null };
export type Draft = {
  id: string;
  body: string;
  branch_id: string | null;
  version: number;
  mentions_json?: string;
};
export type Meta = {
  people: Person[];
  branches: { id: string; display_name: string }[];
  drafts: Draft[];
  tasks: {
    id: string;
    title: string;
    state: string;
    assignee_user_id: string;
    due_at: number;
  }[];
};
