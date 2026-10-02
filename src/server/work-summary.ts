import "server-only";
import { publicScope, type WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { workspaceBase } from "@/lib/workspace";
import { workSnapshot, nextWorkSteps, type WorkSnapshot } from "./work-items";
import { getWorkCapabilities } from "./work-capabilities";
import { buildWorkCards } from "./work-cards";
import { readWorkCardPreferences } from "./work-card-preferences";
import { projectWorkPage, type WorkPageQuery } from "@/lib/work-page";

export async function getWorkCardProjection(actor: WorkContext, work: WorkSnapshot | Promise<WorkSnapshot>) {
  const { write } = await holdingResources(actor.holdingId);
  const [snapshot, capabilities, saved] = await Promise.all([
    work,
    getWorkCapabilities(write, actor),
    readWorkCardPreferences(write, actor)
      .then((preferences) => ({ preferences, preferencesUnavailable: false }))
      .catch(() => ({
        preferences: { pinnedIds: null, version: 0 },
        preferencesUnavailable: true,
      })),
  ]);
  return {
    ...saved,
    capabilities,
    cards: buildWorkCards(
      workspaceBase(publicScope(actor)),
      snapshot,
      capabilities,
    ),
  };
}
export async function getWorkSummary(actor: WorkContext) {
  const workPending = workSnapshot(actor);
  const [work, projection] = await Promise.all([workPending, getWorkCardProjection(actor, workPending)]);
  return {
    ...work,
    ...projection,
    generatedAt: Date.now(),
    nextSteps: nextWorkSteps(work.items),
  };
}
export type WorkSummary = Awaited<ReturnType<typeof getWorkSummary>>;

/** Keep the complete policy snapshot on the server; send only this view's rows. */
export async function getWorkPage(actor: WorkContext, query: WorkPageQuery) {
  return projectWorkPage(await getWorkSummary(actor), query);
}
export type WorkPageSummary=Awaited<ReturnType<typeof getWorkPage>>;
