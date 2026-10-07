import {appAttachmentSources} from "@/server/modules/files/public/app-attachment-sources";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { attachmentContent } from "@/server/modules/files/public/attachments/service";

export async function GET(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/attachments/[id]/content">,
) {
  try {
    const { holdingSlug, id } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write, files } = await holdingResources(actor.holdingId);
    const canonicalLinks=await appAttachmentSources(write).canonicalLinks(id,actor.companyId);
    if(canonicalLinks.length){
      const preview = new URL(request.url).searchParams.has("page") || new URL(request.url).searchParams.get("variant") === "preview";
      let denial:unknown;
      for(const link of canonicalLinks)try{
        const result=await attachmentContent(write,files,actor,link.id,preview?"preview":"original",{compatibilityAdapter:true});
        return new Response(result.object.body,{headers:{"Content-Type":result.mediaType,"Content-Disposition":`${preview?"inline":"attachment"}; filename*=UTF-8''${encodeURIComponent(result.filename)}`,"Cache-Control":"private, no-store","X-Content-Type-Options":"nosniff","Content-Security-Policy":"default-src 'none'; sandbox","Accept-Ranges":"none"}});
      }catch(error){denial=error;if(!(error instanceof ApiError)||![403,404].includes(error.status))throw error;}
      throw denial??new ApiError(404,"NOT_FOUND","Attachment not found.");
    }
    throw new ApiError(
      404,
      "NOT_FOUND",
      "Attachment not found through an authorized file link.",
    );
  } catch (error) {
    return errorResponse(error);
  }
}
