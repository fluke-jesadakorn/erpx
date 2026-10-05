import { expect, it } from "vitest";
import { buildWorkCards } from "../src/server/work-cards";
import {
  defaultWorkCardIds,
  newWorkHref,
  type WorkCapabilities,
} from "../src/lib/work-cards";
import { workCardPreferenceSchema } from "../src/server/work-card-preferences";
import { filterWork, workInView, workTypes, type WorkItem } from "../src/lib/work-item";
import { workCardTone, workTypeTone } from "../src/lib/work-category-tone";
import { betaDestinationAllowed } from "../src/server/beta-access";

const base = "/holdings/acme/branches/bangkok";
const capabilities: WorkCapabilities = {
  creation: [
    {
      kind: "expense",
      label: "Expense claim",
      href: `${base}/work?new=expense`,
    },
    {
      kind: "service_request",
      label: "Service request",
      href: `${base}/work?new=service_request`,
    },
  ],
  teamQueue: false,
  unavailable: [],
};
function item(id: string, extra: Partial<WorkItem> = {}): WorkItem {
  return {
    id,
    type: "expense",
    title: id,
    detail: "Review",
    href: `/expenses/${id}`,
    status: "draft",
    queue: "todo",
    updatedAt: 1,
    ...extra,
  };
}
it("shows draft-capable cards with zero work and omits actions that cannot be used", () => {
  const cards = buildWorkCards(
    base,
    { items: [], unavailableSources: [] },
    capabilities,
  );
  expect(cards.map((c) => c.id)).toEqual(["expenses", "services"]);
  expect(cards[0]).toMatchObject({
    toDo: 0,
    waiting: 0,
    incomplete: false,
    create: { kind: "expense" },
  });
});
it("offers My shifts only with roster access and includes it among default pins", () => {
  const hidden = buildWorkCards(base, { items: [], unavailableSources: [] }, capabilities);
  expect(hidden.some((card) => card.id === "shifts")).toBe(false);
  const visible = buildWorkCards(base, { items: [], unavailableSources: [] }, { ...capabilities, rosterAccess: true });
  expect(visible.find((card) => card.id === "shifts")).toMatchObject({
    href: `${base}/people/roster?view=mine`,
    create: null,
  });
  expect(defaultWorkCardIds(visible)).toContain("shifts");
  expect(workCardTone.shifts).toBe("violet");
});
it("puts the four personal Home actions into authorized default pins", () => {
  const cards = buildWorkCards(base, { items: [], unavailableSources: [] }, {
    ...capabilities,
    rosterAccess: true,
    creation: [...capabilities.creation,
      { kind: "leave", label: "Leave request", href: `${base}/leave` },
      { kind: "overtime", label: "Overtime", href: `${base}/overtime` },
      { kind: "purchase_request", label: "Purchase request", href: `${base}/work?new=purchase_request` },
    ],
  });
  expect(defaultWorkCardIds(cards).slice(0, 4)).toEqual(["shifts", "leave", "overtime", "purchases"]);
});
it("assigns every Work type a stable family tone", () => {
  const ids = workTypes.map(([id]) => id).filter((id) => id !== "all");
  expect(Object.keys(workTypeTone).sort()).toEqual([...ids].sort());
  expect(workTypeTone.expense).toBe("amber");
  expect(workTypeTone.service_request).toBe("teal");
  expect(workTypeTone.leave).toBe("violet");
  expect(workTypeTone.purchase_request).toBe("green");
  expect(workTypeTone.crm_lead).toBe("blue");
});
it("counts deduplicated records and matches each card's canonical destination filters", () => {
  const items = [
    item("a", { relationships: ["requested"] }),
    item("a", { relationships: ["approval"] }),
    item("b", { queue: "waiting", blocker: "Missing evidence" }),
    item("c", { type: "staff_task", relationships: ["team"] }),
  ];
  const cards = buildWorkCards(
    base,
    { items, unavailableSources: [] },
    { ...capabilities, teamQueue: true },
  );
  expect(cards.find((c) => c.id === "expenses")).toMatchObject({
    toDo: 1,
    waiting: 1,
    blocked: 1,
  });
  expect(cards.find((c) => c.id === "tasks")?.create).toBeNull();
  for (const card of cards) {
    const url = new URL(card.href, "http://localhost:3100");
    expect(url.hash).toBe(card.id==="approvals"?"":"#work-results");
    expect(url.searchParams.has("q")).toBe(false);
    const matched = card.id === "approvals" ? workInView([items[1],items[2],items[3]],"approvals") : filterWork(
      [items[1], items[2], items[3]],
      url.searchParams.get("queue")!,
      url.searchParams.get("type") ?? "all",
      "",
      url.searchParams.get("responsibility") ?? "all",
    );
    expect(matched.filter((i) => i.queue === "todo")).toHaveLength(card.toDo!);
    expect(matched.filter((i) => i.queue === "waiting")).toHaveLength(
      card.waiting!,
    );
  }
});
it("limits approvals to actionable decisions and team cards to current membership", () => {
  const items = [
    item("past", { queue: "completed", relationships: ["approval"] }),
    item("team", { relationships: ["team"] }),
  ];
  expect(
    buildWorkCards(base, { items, unavailableSources: [] }, capabilities).some(
      (c) => ["approvals", "team"].includes(c.id),
    ),
  ).toBe(false);
  const available = buildWorkCards(
    base,
    {
      items: [item("decision", { relationships: ["approval"] })],
      unavailableSources: [],
    },
    { ...capabilities, teamQueue: true },
  );
  expect(available.find((c) => c.id === "approvals")?.toDo).toBe(1);
  expect(available.find((c) => c.id === "team")?.toDo).toBe(0);
});
it("marks a failed source as incomplete instead of reporting a misleading zero", () => {
  const cards = buildWorkCards(
    base,
    {
      items: [
        item("approval", { type: "staff_task", relationships: ["approval"] }),
      ],
      unavailableSources: ["Expense claims"],
    },
    capabilities,
  );
  expect(cards.find((c) => c.id === "expenses")).toMatchObject({
    toDo: null,
    waiting: null,
    incomplete: true,
  });
  expect(cards.find((c) => c.id === "approvals")).toMatchObject({
    toDo: null,
    waiting: null,
    incomplete: true,
  });
  expect(cards.find((c) => c.id === "services")).toMatchObject({
    toDo: 0,
    incomplete: false,
  });
});
it("prioritizes pending approvals and team queues before the fixed default order", () => {
  const cards = buildWorkCards(
    base,
    {
      items: [
        item("review", { relationships: ["approval"] }),
        item("team", { queue: "waiting", relationships: ["team"] }),
      ],
      unavailableSources: [],
    },
    { ...capabilities, teamQueue: true },
  );
  expect(defaultWorkCardIds(cards)).toEqual([
    "approvals",
    "team",
    "expenses",
    "services",
  ]);
});
it("preserves return filters on creation and keeps beta conversation creation on Home", () => {
  const href = newWorkHref(
    base,
    capabilities.creation[0],
    "queue=waiting&type=service_request&responsibility=requested&q=receipt",
  );
  const query = new URL(href, "http://localhost:3100").searchParams;
  expect(new URL(href, "http://localhost:3100").pathname).toBe(`${base}/expenses/new`);
  expect(Object.fromEntries(new URLSearchParams(query.get("return") ?? ""))).toEqual({
    queue: "waiting",
    type: "service_request",
    responsibility: "requested",
    q: "receipt",
  });
  const conversation = {
    kind: "conversation" as const,
    label: "Start from a conversation",
    href: `${base}/home`,
  };
  const cards = buildWorkCards(
    base,
    { items: [], unavailableSources: [] },
    { ...capabilities, creation: [conversation] },
  );
  expect(cards[0].id).toBe("tasks");
  expect(newWorkHref(base, cards[0].create!, "queue=all")).toBe(`${base}/home`);
  expect(betaDestinationAllowed("work/card-preferences", true)).toBe(true);
  expect(betaDestinationAllowed("work/card-preferences/admin", true)).toBe(
    false,
  );
});
it("validates bounded unique catalog IDs, own-user input, and default versus empty arrangements", () => {
  const parse = (pinnedIds: unknown, extra = {}) =>
    workCardPreferenceSchema.safeParse({
      pinnedIds,
      expectedVersion: 0,
      ...extra,
    }).success;
  expect(parse(null)).toBe(true);
  expect(parse([])).toBe(true);
  expect(parse(["expenses", "expenses"])).toBe(false);
  expect(parse(["https://evil.example"])).toBe(false);
  expect(
    parse([
      "tasks",
      "expenses",
      "leave",
      "services",
      "equipment",
      "purchases",
      "counts",
    ]),
  ).toBe(false);
  expect(parse(["expenses"], { userId: "someone-else" })).toBe(false);
  expect(parse(["expenses"], { companyId: "other" })).toBe(false);
  expect(parse(["expenses"], { expectedVersion: -1 })).toBe(false);
});
