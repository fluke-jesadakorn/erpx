import { pageError } from "@/server/modules/work_projection/public/page-error";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { SocialPage } from "@/components/collaboration/social-page";
export default async function Post({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params;try{return await SocialPage({actor:await requireWorkContext(holdingSlug),postId:id});}catch(e){return pageError(e, holdingSlug);}}
