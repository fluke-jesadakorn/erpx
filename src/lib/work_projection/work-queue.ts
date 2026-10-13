export type WorkRow = {
  id: string;
  number: string;
  state: string;
  start_date: string;
  days: number;
  created_by: string;
  updated_at: number;
  handover_note: string;
};
export type Queue = "todo" | "waiting" | "completed" | "all";
export const queues = [
  ["todo", "To do"],
  ["waiting", "Waiting"],
  ["completed", "Completed"],
  ["all", "All work"],
] as const;
export function queueFor(
  row: WorkRow,
  userId: string,
  canApprove: boolean,
): Exclude<Queue, "all"> | null {
  if (["approved", "rejected", "cancelled"].includes(row.state))
    return "completed";
  if (row.state === "draft" && row.created_by === userId) return "todo";
  if (row.state === "pending_approval")
    return canApprove && row.created_by !== userId ? "todo" : "waiting";
  return null;
}
