import "server-only";
import type { FlowKind } from "@erpx/feature-shared/lib/flow";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { resolveFlow } from "@erpx/server-workflow/public/flow-routing";

/** Resolve only after the caller has authorized access to the document. */
export async function documentFlow(actor: WorkContext, kind: FlowKind, id: string) {
  return resolveFlow((await holdingResources(actor.holdingId)).write, actor, kind, id);
}
