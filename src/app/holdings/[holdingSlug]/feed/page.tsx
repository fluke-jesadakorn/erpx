import { redirect } from "next/navigation";
import { publicScope,requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
export default async function Updates({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const actor=await requireWorkContext((await params).holdingSlug),query=await searchParams;const post=typeof query.post==="string"?query.post:typeof query.postId==="string"?query.postId:null;redirect(`${workspaceBase(publicScope(actor))}/${post?`posts/${encodeURIComponent(post)}`:"home"}`);}
