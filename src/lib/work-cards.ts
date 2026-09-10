import type { WorkItem } from "./work-item";

export const workCardIds = [
  "tasks",
  "expenses",
  "leave",
  "services",
  "equipment",
  "purchases",
  "approvals",
  "team",
  "counts",
  "transfers",
] as const;
export type WorkCardId = (typeof workCardIds)[number];
export type NewWorkKind =
  | "expense"
  | "service_request"
  | "equipment_request"
  | "purchase_request"
  | "task"
  | "leave"
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
    id: "leave",
    title: "Leave requests",
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
export function defaultWorkCardIds(cards: WorkActionCard[]): WorkCardId[] {
  const pending = ["approvals", "team"].filter((id) =>
    cards.some((c) => c.id === id && (c.toDo ?? 0) + (c.waiting ?? 0) > 0),
  );
  const order = [
    ...pending,
    "expenses",
    "services",
    "tasks",
    "leave",
    "equipment",
    "purchases",
    "counts",
    "transfers",
    "team",
  ];
  return [...new Set(order)]
    .filter((id) => cards.some((c) => c.id === id))
    .slice(0, 6) as WorkCardId[];
}
export function newWorkHref(
  base: string,
  creation: WorkCreation,
  current: string,
) {
  if (creation.kind === "conversation") return creation.href;
  const query = new URLSearchParams(current);
  query.set("new", creation.kind);
  return `${base}/work?${query}`;
}
