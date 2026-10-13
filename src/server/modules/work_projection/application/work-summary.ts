import type {WorkContext,publicScope} from "../../identity_access/public/authorization";
import type {WorkCapabilities,WorkCardPreferences} from "@/lib/work_projection/work-cards";
import type {Clock} from "../../shared/application/ports";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {projectWorkPage,type WorkPageQuery} from "@/lib/work_projection/work-page";
import {nextWorkSteps,type WorkSnapshot} from "../domain/work-items";
import {buildWorkCards,buildDestinationCards} from "../domain/work-cards";
import type {navigationContext} from "../public/navigation-context";
export type WorkSummaryPorts={clock:Clock;scope:typeof publicScope;resource(actor:WorkContext):Promise<{capabilities(actor:WorkContext):Promise<WorkCapabilities>;preferences(actor:WorkContext):Promise<WorkCardPreferences>}>;navigation:typeof navigationContext;snapshot(actor:WorkContext):Promise<WorkSnapshot>};
export function createWorkSummary(ports:WorkSummaryPorts){
async function getWorkCardProjection(actor: WorkContext, work: WorkSnapshot | Promise<WorkSnapshot>) {
  const unit=await ports.resource(actor);
  const [snapshot, capabilities, saved, navigation] = await Promise.all([
    work,
    unit.capabilities(actor),
    unit.preferences(actor)
      .then((preferences) => ({ preferences, preferencesUnavailable: false }))
      .catch(() => ({
        preferences: { pinnedIds: null, version: 0 },
        preferencesUnavailable: true,
      })),
    ports.navigation(actor),
  ]);
  return {
    ...saved,
    capabilities,
    cards: [...buildWorkCards(
      workspaceBase(ports.scope(actor)),
      snapshot,
      capabilities,
    ), ...buildDestinationCards(navigation)],
  };
}
async function getWorkSummary(actor: WorkContext) {
  const workPending = ports.snapshot(actor);
  const [work, projection] = await Promise.all([workPending, getWorkCardProjection(actor, workPending)]);
  return {
    ...work,
    ...projection,
    generatedAt: ports.clock.now(),
    nextSteps: nextWorkSteps(work.items,ports.clock.now()),
  };
}


/** Keep the complete policy snapshot on the server; send only this view's rows. */
async function getWorkPage(actor: WorkContext, query: WorkPageQuery) {
  return projectWorkPage(await getWorkSummary(actor), query);
}


return {getWorkCardProjection,getWorkSummary,getWorkPage};}
export type WorkSummary=Awaited<ReturnType<ReturnType<typeof createWorkSummary>["getWorkSummary"]>>;
export type WorkPageSummary=Awaited<ReturnType<ReturnType<typeof createWorkSummary>["getWorkPage"]>>;
