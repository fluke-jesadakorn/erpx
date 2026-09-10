import { requireWorkContext } from "@/server/authorization";
import { SocialPage } from "@/components/beta/social-page";
import { notFound } from "next/navigation";
import { ApiError } from "@/server/http";
export default async function Post({params}:{params:Promise<{holdingSlug:string;id:string}>}){const {holdingSlug,id}=await params;try{return await SocialPage({actor:await requireWorkContext(holdingSlug),postId:id});}catch(e){if(e instanceof ApiError&&e.status===404)notFound();throw e;}}
