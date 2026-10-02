import { compareWork, isMyAction, type WorkItem } from "./work-item";

const financeTypes = new Set<WorkItem["type"]>(["invoice", "journal", "expense", "supplier_bill", "supplier_batch", "refund", "payroll"]);
export type FinanceWorkView = "actions" | "waiting" | "team";

/** Use source-owned authority and record identity; queue counters are not tasks. */
export function financeWork(items: readonly WorkItem[], view: FinanceWorkView, query = "") {
  const rows = items.filter(item => financeTypes.has(item.type) && item.queue !== "completed")
    .filter(item => view === "actions" ? isMyAction(item) : view === "waiting" ? item.queue === "waiting" && !!item.relationships?.some(value => value !== "team") : item.relationships?.includes("team") || item.category === "setup")
    .filter(item => `${item.title} ${item.detail} ${item.nextAction ?? ""}`.toLowerCase().includes(query.trim().toLowerCase()))
    .sort(compareWork);
  const identities = new Set<string>();
  return rows.filter(item => { const key=`${item.type}:${item.id}`; if(identities.has(key))return false;identities.add(key);return true; });
}
