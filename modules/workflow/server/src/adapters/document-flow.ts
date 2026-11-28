import "server-only";
import type { FlowKind } from "@erpx/platform-shared-ui/lib/flow";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import {resolveFlow} from '@erpx/workflow-server/public/flow-routing';

/** Resolve only after the caller has authorized access to the document. */
export async function documentFlow(actor: WorkContext, kind: FlowKind, id: string) {
  return resolveFlow((await holdingResources(actor.holdingId)).write, actor, kind, id);
}
