export const serviceCategories = [
  ["it", "IT support"],
  ["engineering", "Engineering"],
  ["maintenance", "Maintenance"],
  ["facilities", "Facilities"],
  ["housekeeping", "Housekeeping"],
  ["people", "People & HR"],
  ["finance", "Accounts & Finance"],
  ["purchasing", "Purchasing"],
  ["operations", "Warehouse & Operations"],
  ["sales", "Sales & Customer Support"],
] as const;
export type ServiceCategory = (typeof serviceCategories)[number][0];
export const requestKinds = [
  ["service", "Service request"],
  ["equipment", "Equipment request"],
  ["purchase", "Purchase request"],
] as const;
export type RequestKind = (typeof requestKinds)[number][0];
export const requestStateLabels: Record<string, string> = {
  draft: "Draft",
  returned: "Needs correction",
  blocked: "Setup needed",
  awaiting_approval: "Department approval",
  queued: "Waiting for team",
  in_progress: "In progress",
  waiting_requester: "Waiting for your reply",
  resolved: "Ready for confirmation",
  fulfilled: "Ready for confirmation",
  closed: "Completed",
  rejected: "Rejected",
  cancelled: "Cancelled",
};
export const categoryLabel = (id: string) =>
  serviceCategories.find(([key]) => key === id)?.[1] ?? id;
export type RequestItem = {
  description: string;
  quantity: string;
  estimatedCost: string;
};
export type RequestFields = {
  kind: RequestKind;
  category: string;
  title: string;
  description: string;
  location: string;
  impact: string;
  branchId: string | null;
  departmentId: string | null;
  neededDate: string;
  items: RequestItem[];
  currency: string;
};
