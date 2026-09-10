import { requireWorkContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { searchReferences } from "@/server/records";
export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/search">){try{const {holdingSlug}=await context.params;const q=new URL(request.url).searchParams.get("q")??"";return Response.json({data:q.length<1?[]:await searchReferences(await requireWorkContext(holdingSlug),q)});}catch(error){return errorResponse(error);}}
