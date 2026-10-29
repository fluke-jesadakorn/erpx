import type { RouteContext } from "@erpx/contracts/next-routes";
import {nativeFieldAttachment} from "@erpx/server-files/public/attachments/native-field-attachment";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { attachmentLinkStatus } from "@erpx/server-files/public/attachments/service";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../../../../../../../../packages/server-runtime/src/public/runtime";
export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/attachment-links/[linkId]/status">){try{const {holdingSlug,linkId}=await context.params,actor=await requireWorkContext(holdingSlug),native=await nativeFieldAttachment(request.headers,actor,linkId);if(native)return Response.json(canonicalDestinationLinks({data:native}),{headers:{"Cache-Control":"private, no-store"}});const {write}=await holdingResources(actor.holdingId);return Response.json(canonicalDestinationLinks({data:await attachmentLinkStatus(write,actor,linkId)}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
