import "server-only";
import type { FlowKind } from "@/lib/flow";
import type { WorkContext } from "../../identity_access/public/authorization";
import { holdingResources } from "../../runtime/public/runtime";
import { resolveFlow } from "../public/flow-routing";

/** Resolve only after the caller has authorized access to the document. */
export async function documentFlow(actor: WorkContext, kind: FlowKind, id: string) {
  return resolveFlow((await holdingResources(actor.holdingId)).write, actor, kind, id);
}
