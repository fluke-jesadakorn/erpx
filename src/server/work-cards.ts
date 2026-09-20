import { selfServiceHref } from "@/lib/self-service";
import "server-only";
import {
  workCardCatalog,
  type WorkActionCard,
  type WorkCapabilities,
} from "@/lib/work-cards";
import { mergeWorkItems, type WorkSnapshot } from "./work-items";

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
      card.type
        ? i.type === card.type
        : card.id === "approvals"
          ? i.queue === "todo" && i.relationships?.includes("approval")
          : i.relationships?.includes("team"),
    );
    const create =
      capabilities.creation.find(
        (c) =>
          c.kind === card.newKind ||
          (card.id === "tasks" && c.kind === "conversation"),
      ) ?? null;
    const visible =
      card.id === "team"
        ? capabilities.teamQueue
        : !!create || matches.length > 0;
    if (!visible) return [];
    const incomplete = card.sources.some((s) => unavailable.has(s));
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
          ["leave","overtime"].includes(card.id)
            ? selfServiceHref(base,card.id)!
            : `${base}/work?${query}#work-results`,
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
