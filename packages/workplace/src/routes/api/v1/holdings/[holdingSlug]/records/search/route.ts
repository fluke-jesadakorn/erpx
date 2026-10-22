import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { searchReferences } from "@erpx/server-records/public/records";
export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/search">){try{const {holdingSlug}=await context.params;const q=new URL(request.url).searchParams.get("q")??"";return Response.json(canonicalDestinationLinks({data:q.length<1?[]:await searchReferences(await requireWorkContext(holdingSlug),q)}));}catch(error){return errorResponse(error);}}
