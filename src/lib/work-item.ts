export type WorkItem = {
  id: string;
  type:
    | "leave"
    | "expense"
    | "stock_count"
    | "transfer"
    | "staff_task"
    | "service_request"
    | "equipment_request"
    | "purchase_request";
  title: string;
  detail: string;
  href: string;
  status: string;
  updatedAt: number;
  queue: "todo" | "waiting" | "completed";
  responsibleQueue?: string;
  nextAction?: string;
  dueAt?: number | null;
  blocker?: string | null;
  relationships?: ("requested" | "assigned" | "approval" | "team")[];
};
export const workTypes = [
  ["all", "All types"],
  ["staff_task", "Tasks"],
  ["leave", "Leave requests"],
  ["expense", "Expense claims"],
  ["stock_count", "Stock counts"],
  ["transfer", "Warehouse transfers"],
  ["service_request", "Service requests"],
  ["equipment_request", "Equipment requests"],
  ["purchase_request", "Purchase requests"],
] as const;
export const workResponsibilities = [
  ["all", "All responsibilities"],
  ["requested", "My requests"],
  ["assigned", "Assigned to me"],
  ["approval", "Approvals"],
  ["team", "Team queue"],
] as const;
export function filterWork(
  items: WorkItem[],
  queue: string,
  type: string,
  q: string,
  responsibility = "all",
) {
  return items.filter(
    (item) =>
      (queue === "all" || item.queue === queue) &&
      (type === "all" || item.type === type) &&
      (responsibility === "all" ||
        item.relationships?.some((r) => r === responsibility)) &&
      `${item.title} ${item.detail} ${item.status}`
        .toLowerCase()
        .includes(q.toLowerCase()),
  );
}
