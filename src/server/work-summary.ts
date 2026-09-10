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
