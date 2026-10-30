import {boundedJsonBody} from '../../../../../../../../lib/bounded-json-body';
import {nativeFieldFamilyCommand} from '../../../../../../../../lib/native-field-family';
import {fieldFileWriterOwner} from '@erpx/server-files/public/field-file-writer-owner';
import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {appAttachmentSources} from "@erpx/server-files/public/app-attachment-sources";
import { z } from "zod";
import { bindRecordAttachmentSet } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";

const schema = z.object({ expectedTargetVersion: z.number().int().positive() });

export async function POST(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-sets/[setId]/bind">,
) {
  try {
    const origin = request.headers.get("origin");
    if (origin && origin !== requestPublicOrigin(request))
      throw new ApiError(403, "ORIGIN_FORBIDDEN", "Use the application origin.");
    const key = request.headers.get("Idempotency-Key");
    if (!key || !/^[a-zA-Z0-9:_.-]{8,180}$/.test(key))
      throw new ApiError(
        422,
        "COMMAND_KEY_REQUIRED",
        "A valid Idempotency-Key is required.",
      );
    const { holdingSlug, setId } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const body = await boundedJsonBody(request,2*1024*1024);
    const parsed = schema.safeParse(body.value);
    if (!parsed.success)
      throw new ApiError(
        422,
        "VALIDATION_FAILED",
        parsed.error.issues[0]?.message ?? "Invalid record document bind.",
      );
    if(await fieldFileWriterOwner(actor,{kind:"set",id:setId})==="field")return await nativeFieldFamilyCommand(request,actor,"bind",setId,{key,body:new TextDecoder().decode(body.raw),expectedVersion:parsed.data.expectedTargetVersion});
    const { write } = await holdingResources(actor.holdingId);
    const compatibilityAdapter = await appAttachmentSources(write).compatibilitySet(setId,actor.companyId);
    return Response.json(canonicalDestinationLinks({
      data: await bindRecordAttachmentSet(
        write,
        actor,
        setId,
        parsed.data.expectedTargetVersion,
        key,
        { compatibilityAdapter },
      ),
    }));
  } catch (error) {
    return errorResponse(error);
  }
}
