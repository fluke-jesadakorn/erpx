import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { iamActorFromWork } from "@erpx/server-identity_access/public/iam-context";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import {
  technologyData,
  previewProvisioning,
  applyProvisioning,
  retryProvisioning,
} from "@erpx/server-identity_access/public/technology";
import { provisioningInput } from "@erpx/server-identity_access/public/rules/technology";
import { directoryOperationsApi } from "@erpx/server-identity_access/public/directory-operations-api";
import { positionGpoOverview, previewPositionGpo, publishPositionGpo, previewUnmapPositionGpo, unmapPositionGpo, retryPositionGpo, retryPositionGpoConfig } from "@erpx/server-identity_access/public/position-gpo";
import { gpoRulesOverview, previewGpoRule, publishGpoRule, previewRemoveGpoRule, removeGpoRule, retryGpoRuleConfig, markPositionGpoNotRequired } from "@erpx/server-identity_access/public/gpo-rules";
import { organizationAccessData } from "@erpx/server-organization/public/organization-access";
import {bindProvisioningEvidence} from "@erpx/server-platform_administration/public/system-evidence";
type Context = { params: Promise<{ holdingSlug: string; path?: string[] }> };
export const dynamic = "force-dynamic";
export async function GET(_request: Request, context: Context) {
  try {
    const { holdingSlug,path=[] } = await context.params,
      actor = await requireWorkContext(holdingSlug),
      { write } = await holdingResources(actor.holdingId);
    return Response.json(
      canonicalDestinationLinks({
        data: path[0] === "organization" ? await organizationAccessData(write, actor)
          : path[0] === "gpo-rules" ? await gpoRulesOverview(write as PostgresDatabase, actor)
          : path[0]==="position-gpo" ? await positionGpoOverview(write as PostgresDatabase,actor) : await technologyData(
          write as PostgresDatabase,
          iamActorFromWork(actor),
        ),
      }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(
        403,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
      );
    const key = request.headers.get("Idempotency-Key");
    if (!key)
      throw new ApiError(
        422,
        "IDEMPOTENCY_KEY_REQUIRED",
        "Idempotency-Key is required.",
      );
    const { holdingSlug, path = [] } = await context.params,
      contextActor = await requireWorkContext(holdingSlug),
      actor = iamActorFromWork(contextActor),
      { write } = await holdingResources(contextActor.holdingId),
      db = write as PostgresDatabase,
      raw = await request.json();
    if(path[0]==='provisioning'&&path[1]&&path[2]==='evidence'&&path.length===3)
      return Response.json(canonicalDestinationLinks({data:await bindProvisioningEvidence(db,contextActor,path[1],raw,key)}));
    if (path[0] === "directory")
      return Response.json(canonicalDestinationLinks({
        data: await directoryOperationsApi(
          db,
          contextActor,
          path.slice(1),
          raw,
          key,
        ),
      }));
    if (path[0] === "gpo-rules") {
      if (path[1] === "not-required") return Response.json(canonicalDestinationLinks({ data: await markPositionGpoNotRequired(db, contextActor, raw, key) }));
      if (path[1] === "preview") return Response.json(canonicalDestinationLinks({ data: await previewGpoRule(db, contextActor, raw) }));
      if (path[1] === "publish") return Response.json(canonicalDestinationLinks({ data: await publishGpoRule(db, contextActor, raw, key) }));
      if (path[1] === "remove-preview") return Response.json(canonicalDestinationLinks({ data: await previewRemoveGpoRule(db, contextActor, raw) }));
      if (path[1] === "remove") return Response.json(canonicalDestinationLinks({ data: await removeGpoRule(db, contextActor, raw, key) }));
      if (path[1] === "retry-config") return Response.json(canonicalDestinationLinks({ data: await retryGpoRuleConfig(db, contextActor, raw, key) }));
      throw new ApiError(404, "NOT_FOUND", "GPO rule operation not found.");
    }
    if(path[0]==="position-gpo" && path[1]==="preview")return Response.json(canonicalDestinationLinks({data:await previewPositionGpo(db,contextActor,raw)}));
    if(path[0]==="position-gpo" && path[1]==="publish")return Response.json(canonicalDestinationLinks({data:await publishPositionGpo(db,contextActor,raw,key)}));
    if(path[0]==="position-gpo" && path[1]==="unmap-preview")return Response.json(canonicalDestinationLinks({data:await previewUnmapPositionGpo(db,contextActor,raw)}));
    if(path[0]==="position-gpo" && path[1]==="unmap")return Response.json(canonicalDestinationLinks({data:await unmapPositionGpo(db,contextActor,raw,key)}));
    if(path[0]==="position-gpo" && path[1]==="retry")return Response.json(canonicalDestinationLinks({data:await retryPositionGpo(db,contextActor,raw,key)}));
    if(path[0]==="position-gpo" && path[1]==="retry-config")return Response.json(canonicalDestinationLinks({data:await retryPositionGpoConfig(db,contextActor,raw,key)}));
    if (path[0] === "provisioning" && path[1] === "preview")
      return Response.json(canonicalDestinationLinks({
        data: await previewProvisioning(
          db,
          actor,
          provisioningInput.parse(raw),
        ),
      }));
    if (path[0] === "provisioning" && path[1] === "retry")
      return Response.json(canonicalDestinationLinks({
        data: await retryProvisioning(
          db,
          actor,
          z.string().parse(raw.id),
          z.number().int().parse(raw.expectedVersion),
          key,
        ),
      }));
    if (path[0] === "provisioning" && path[1] === "apply")
      return Response.json(canonicalDestinationLinks({
        data: await applyProvisioning(
          db,
          actor,
          provisioningInput.parse(raw),
          z.string().parse(raw.previewToken),
          key,
        ),
      }));
    throw new ApiError(404, "NOT_FOUND", "Technology operation not found.");
  } catch (e) {
    return errorResponse(e);
  }
}
