import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { ApiError, errorResponse } from "@/server/http";
import { attachmentContent } from "@/server/attachments/service";

export async function GET(
  request: Request,
  context: RouteContext<"/api/v1/holdings/[holdingSlug]/attachments/[id]/content">,
) {
  try {
    const { holdingSlug, id } = await context.params;
    const actor = await requireWorkContext(holdingSlug);
    const { write, files } = await holdingResources(actor.holdingId);
    const canonicalLinks=(await write.prepare(`SELECT l.id FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id
      WHERE l.attachment_id=? AND s.company_id=? AND l.state IN ('draft','bound') ORDER BY CASE l.state WHEN 'bound' THEN 0 ELSE 1 END,l.created_at`)
      .bind(id,actor.companyId).all<{id:string}>()).results;
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
