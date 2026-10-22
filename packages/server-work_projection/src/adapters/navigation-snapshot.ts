import "server-only";
import { cache } from "react";
import { workAttention } from "@erpx/feature-work_projection/lib/work-attention";
import { isMyAction, workMatchesView } from "@erpx/feature-work_projection/lib/work-item";
import { requireWorkContext, type WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { workSnapshot } from "@erpx/server-work_projection/public/work-items";
import { getWorkCapabilities } from "@erpx/server-work_projection/public/work-capabilities";
import { unreadHomeCount } from "@erpx/server-work_projection/public/navigation-counts";
import { measureReadPhase } from "@erpx/server-runtime/public/runtime/read-metrics";

const navigationRead = cache((actor: WorkContext, includeAttention = true) => {
  let workPending!: ReturnType<typeof workSnapshot>;
  const data = measureReadPhase("navigation", async () => {
    workPending = workSnapshot(actor);
    const { write } = await holdingResources(actor.holdingId);
    const [home, work, capabilities] = await Promise.all([
      unreadHomeCount(write, actor), workPending, getWorkCapabilities(write, actor),
    ]);
    return {
      home,
      work: work.unavailableSources.length || includeAttention && capabilities.unavailable.length ? null : work.items.filter(isMyAction).length,
      ...(includeAttention ? {
        attention: workAttention(work.items, !!work.unavailableSources.length || !!capabilities.unavailable.length),
        snapshotAt: Date.now(),
      } : {}),
      workViewAvailability: {
        team: capabilities.teamQueue || work.items.some(item => item.relationships?.includes("team")),
        routing: work.items.some(item => workMatchesView(item, "routing")),
      },
    };
  });
  return { work: workPending, data };
});

export function navigationSnapshot(actor: WorkContext, includeAttention = true) {
  return navigationRead(actor, includeAttention).data;
}

/** One authorized context and read shared by the Home page and its layout. */
const workspaceReads = cache(() => new Map<string, ReturnType<typeof loadWorkspaceNavigation>>());
export function workspaceNavigation(params: string | Promise<{ holdingSlug: string }>) {
  // Capture React's request cache synchronously, before awaiting route params.
  const reads = workspaceReads();
  return Promise.resolve(params).then(value => {
    const holdingSlug = typeof value === "string" ? value : value.holdingSlug;
    let pending = reads.get(holdingSlug);
    if (!pending) {
      pending = loadWorkspaceNavigation(holdingSlug);
      reads.set(holdingSlug, pending);
    }
    return pending;
  });
}
async function loadWorkspaceNavigation(holdingSlug: string) {
  const context = await measureReadPhase("work_context", () => requireWorkContext(holdingSlug));
  const read = navigationRead(context);
  return { context, initialNavigation: read.data.catch(() => null), initialWork: read.work };
}
export type NavigationSnapshot = Awaited<ReturnType<typeof navigationSnapshot>>;

/** Included source pages keep the current work role, without reading or
 * serializing Work attention, counts or unrelated owner snapshots. */
export async function restrictedWorkspaceNavigation(holdingSlug:string){
 const context=await measureReadPhase("work_context",()=>requireWorkContext(holdingSlug));
 return {context,initialNavigation:undefined};
}
