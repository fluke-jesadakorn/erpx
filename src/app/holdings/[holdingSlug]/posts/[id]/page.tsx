import { pageError } from "@/server/page-error";
import { requireWorkContext } from "@/server/authorization";
import { SocialPage } from "@/components/beta/social-page";
export default async function Post({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params;try{return await SocialPage({actor:await requireWorkContext(holdingSlug),postId:id});}catch(e){return pageError(e, holdingSlug);}}
