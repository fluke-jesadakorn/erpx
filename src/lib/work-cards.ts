import { requestAction } from "./request-actions";
import { selfServiceHref } from "./self-service";
import type { WorkItem } from "./work-item";
import { shortcutDestinationIds } from "./workplace-catalogue";

export const workCardIds = [
  "tasks",
  "expenses",
  "shifts",
  "leave",
  "overtime",
  "services",
  "equipment",
  "purchases",
  "approvals",
  "team",
  "counts",
  "transfers",
  ...shortcutDestinationIds,
] as const;
export type WorkCardId = (typeof workCardIds)[number];
export type NewWorkKind =
  | "expense"
  | "service_request"
  | "equipment_request"
  | "purchase_request"
  | "task"
  | "leave"
  | "overtime"
  | "stock_count"
  | "transfer";
export type WorkCreation = {
  kind: NewWorkKind | "conversation";
  label: string;
  href: string;
};
export type WorkCapabilities = {
  creation: WorkCreation[];
  teamQueue: boolean;
  rosterAccess?: boolean;
  unavailable: string[];
};
export type WorkActionCard = {
  id: WorkCardId;
  title: string;
  icon: WorkCardId;
  href: string;
  create: WorkCreation | null;
  toDo: number | null;
  waiting: number | null;
  blocked: number;
  incomplete: boolean;
  destination?: boolean;
  objective?: string;
};
export type WorkCardPreferences = {
  pinnedIds: WorkCardId[] | null;
  version: number;
};
export const workCardCatalog: {
  id: WorkCardId;
  title: string;
  type?: WorkItem["type"];
  newKind?: NewWorkKind;
  sources: string[];
}[] = [
  {
    id: "tasks",
    title: "Tasks",
    type: "staff_task",
    newKind: "task",
    sources: ["Tasks"],
  },
  {
    id: "expenses",
    title: "Expense claims",
    type: "expense",
    newKind: "expense",
    sources: ["Expense claims"],
  },
  {
    id: "shifts",
    title: "My shifts",
    sources: ["Schedule"],
  },
  {id:"overtime",title:"Overtime",type:"overtime",newKind:"overtime",sources:["Overtime"]},
  {
    id: "leave",
    title: "My leave",
    type: "leave",
    newKind: "leave",
    sources: ["Leave requests"],
  },
  {
    id: "services",
    title: "Service requests",
    type: "service_request",
    newKind: "service_request",
    sources: ["Employee requests"],
  },
  {
    id: "equipment",
    title: "Equipment requests",
    type: "equipment_request",
    newKind: "equipment_request",
    sources: ["Employee requests"],
  },
  {
    id: "purchases",
    title: "Purchase requests",
    type: "purchase_request",
    newKind: "purchase_request",
    sources: ["Employee requests"],
  },
  {
    id: "approvals",
    title: "Approvals",
    sources: [
      "Tasks",
      "Expense claims",
      "Leave requests",
      "Employee requests",
      "Stock counts",
      "Warehouse transfers",
    ],
  },
  {
    id: "team",
    title: "Team queue",
    sources: ["Employee requests", "Team coverage"],
  },
  {
    id: "counts",
    title: "Stock counts",
    type: "stock_count",
    newKind: "stock_count",
    sources: ["Stock counts"],
  },
  {
    id: "transfers",
    title: "Warehouse transfers",
    type: "transfer",
    newKind: "transfer",
    sources: ["Warehouse transfers"],
  },
];
export function defaultWorkCardIds(cards: WorkActionCard[], role = ""): WorkCardId[] {
  const recommended = /sales|commercial/.test(role) ? ["customer-followups", "crm-pipeline", "operations-quotations", "operations-sales"]
    : /warehouse|inventory|stock/.test(role) ? ["operations-transit", "operations-stock", "transfers", "counts"]
    : /finance|treasury|account/.test(role) ? ["finance-payments", "finance-ledger", "finance-bank", "approvals"]
    : /hr|people|payroll/.test(role) ? ["team", "approvals", "shifts", "leave"] : [];
  const pending = ["approvals", "team"].filter((id) =>
    cards.some((c) => c.id === id && (c.toDo ?? 0) + (c.waiting ?? 0) > 0),
  );
  const order = [
    ...recommended,
    ...pending,
    "shifts",
    "leave",
    "overtime",
    "purchases",
    "expenses",
    "services",
    "tasks",
    "equipment",
    "counts",
    "transfers",
    "team",
  ];
  return [...new Set(order)]
    .filter((id) => cards.some((c) => c.id === id))
    .slice(0, 4) as WorkCardId[];
}
export function newWorkHref(
  base: string,
  creation: WorkCreation,
  current: string,
) {
  const request=requestAction(creation.kind);
  if (request) {
    const path=request.path;
    return `${base}${path}${path.includes("?") ? "&" : "?"}${new URLSearchParams({return:current})}`;
  }
  const personal=selfServiceHref(base,creation.kind);
  if(personal)return personal;
  if (creation.kind === "conversation") return creation.href;
  const query = new URLSearchParams(current);
  query.set("new", creation.kind);
  return creation.kind === "expense"
    ? `${base}/expenses/new?${query}`
    : `${base}/work?${query}`;
}
