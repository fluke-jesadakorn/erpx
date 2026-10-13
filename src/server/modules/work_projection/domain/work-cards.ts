import { selfServiceHref } from "@/lib/work_projection/self-service";

import {
  workCardCatalog,
  type WorkActionCard,
  type WorkCapabilities,
} from "@/lib/work_projection/work-cards";
import { mergeWorkItems, type WorkSnapshot } from "./work-items";
import { isApprovalAction } from "@/lib/work_projection/work-item";
import { shortcutDestinationIds } from "@/lib/work_projection/workplace-catalogue";
import { workplaceNavigation, type NavigationContext } from "@/lib/work_projection/workplace-navigation";

export function buildWorkCards(
  base: string,
  snapshot: WorkSnapshot,
  capabilities: WorkCapabilities,
): WorkActionCard[] {
  const items = mergeWorkItems(snapshot.items);
  const unavailable = new Set([
    ...snapshot.unavailableSources,
    ...capabilities.unavailable,
  ]);
  return workCardCatalog.flatMap((card) => {
    const matches = items.filter((i) =>
      card.id === "shifts"
        ? false
        : card.type
        ? i.type === card.type
        : card.id === "approvals"
          ? isApprovalAction(i)
          : i.relationships?.includes("team"),
    );
    const create =
      capabilities.creation.find(
        (c) =>
          c.kind === card.newKind ||
          (card.id === "tasks" && c.kind === "conversation"),
      ) ?? null;
    const visible =
      card.id === "shifts"
        ? !!capabilities.rosterAccess
        : card.id === "team"
        ? capabilities.teamQueue
        : !!create || matches.length > 0;
    if (!visible) return [];
    const incomplete = card.id !== "shifts" && card.sources.some((s) => unavailable.has(s));
    const query = new URLSearchParams(
      card.type
        ? { type: card.type, queue: "all" }
        : {
            responsibility: card.id === "approvals" ? "approval" : "team",
            queue: card.id === "approvals" ? "todo" : "all",
          },
    );
    return [
      {
        id: card.id,
        title: card.title,
        icon: card.id,
        href:
          ["shifts","leave","overtime"].includes(card.id)
            ? selfServiceHref(base,card.id)!
            : card.id === "approvals" ? `${base}/work?view=approvals` : `${base}/work?${query}#work-results`,
        create,
        toDo: incomplete
          ? null
          : matches.filter((i) => i.queue === "todo").length,
        waiting: incomplete
          ? null
          : matches.filter((i) => i.queue === "waiting").length,
        blocked: matches.filter((i) => i.blocker).length,
        incomplete,
      },
    ];
  });
}

export function buildDestinationCards(context: NavigationContext): WorkActionCard[] {
  return workplaceNavigation(context).search.flatMap(item => {
    if (!shortcutDestinationIds.includes(item.id as typeof shortcutDestinationIds[number])) return [];
    return [{ id: item.id as WorkActionCard["id"], title: item.label, icon: item.id as WorkActionCard["icon"],
      href: item.href, objective: item.objective, destination: true, create: null,
      toDo: null, waiting: null, blocked: 0, incomplete: false }];
  });
}
