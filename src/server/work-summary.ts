import "server-only";
import { publicScope, type WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { workspaceBase } from "@/lib/workspace";
import { workSnapshot, nextWorkSteps } from "./work-items";
import { getWorkCapabilities } from "./work-capabilities";
import { buildWorkCards } from "./work-cards";
import { readWorkCardPreferences } from "./work-card-preferences";

export async function getWorkSummary(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  const [work, capabilities, saved] = await Promise.all([
    workSnapshot(actor),
    getWorkCapabilities(write, actor),
    readWorkCardPreferences(write, actor)
      .then((preferences) => ({ preferences, preferencesUnavailable: false }))
      .catch(() => ({
        preferences: { pinnedIds: null, version: 0 },
        preferencesUnavailable: true,
      })),
  ]);
  return {
    ...work,
    ...saved,
    generatedAt: Date.now(),
    capabilities,
    cards: buildWorkCards(
      workspaceBase(publicScope(actor)),
      work,
      capabilities,
    ),
    nextSteps: nextWorkSteps(work.items),
  };
}
export type WorkSummary = Awaited<ReturnType<typeof getWorkSummary>>;

/** Keep the complete policy snapshot on the server; send only this view's rows. */
export async function getWorkPage(actor: WorkContext, query: {
  view:string;queue:string;type:string;q:string;responsibility:string;offset?:number;
}) {
  const { filterWork,workInView,workViews,isMyAction } = await import("@/lib/work-item");
  const summary=await getWorkSummary(actor);
  const selected=filterWork(workInView(summary.items,query.view),query.view==='legacy'?query.queue:'all',query.type,query.q,query.responsibility);
  const requested=Number.isSafeInteger(query.offset)&&query.offset!>0?query.offset!:0;
  const offset=Math.min(requested,Math.max(0,Math.floor((selected.length-1)/30)*30));
  return {...summary,items:selected.slice(offset,offset+30),
    list:{offset,total:selected.length,hasMore:offset+30<selected.length},
    actionCount:summary.unavailableSources.length?null:summary.items.filter(isMyAction).length,
    viewCounts:Object.fromEntries(workViews.map(([id])=>[id,filterWork(workInView(summary.items,id),'all',query.type,query.q,query.responsibility).length])),
    relationships:[...new Set(summary.items.flatMap(i=>i.relationships??[]))],
  };
}
export type WorkPageSummary=Awaited<ReturnType<typeof getWorkPage>>;
