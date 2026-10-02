import { filterWork, isMyAction, workInView, workMatchesView, workViews } from "./work-item";
import type { WorkSummary } from "@/server/work-summary";

export type WorkPageQuery = {
  view: string; queue: string; type: string; q: string; responsibility: string; offset?: number;
};

/** Counts cover the complete policy snapshot; only the requested rows leave the server. */
export function projectWorkPage(summary: WorkSummary, query: WorkPageQuery) {
  const matching = filterWork(summary.items, "all", query.type, query.q, query.responsibility);
  const selected = workInView(query.view === "legacy"
    ? matching.filter(item => query.queue === "all" || item.queue === query.queue)
    : matching, query.view);
  const requested = Number.isSafeInteger(query.offset) && query.offset! > 0 ? query.offset! : 0;
  const offset = Math.min(requested, Math.max(0, Math.floor((selected.length - 1) / 30) * 30));
  return {
    ...summary,
    items: selected.slice(offset, offset + 30),
    list: { offset, total: selected.length, hasMore: offset + 30 < selected.length },
    actionCount: summary.unavailableSources.length ? null : summary.items.filter(isMyAction).length,
    viewCounts: Object.fromEntries(workViews.map(([id]) => [id, matching.filter(item => workMatchesView(item, id)).length])),
    relationships: [...new Set(summary.items.flatMap(item => item.relationships ?? []))],
  };
}
