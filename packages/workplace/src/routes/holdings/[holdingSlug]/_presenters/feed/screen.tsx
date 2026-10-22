import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { redirect } from "next/navigation";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
export default async function Updates({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}){const actor=await requireWorkContext((await params).holdingSlug),query=await searchParams;const post=typeof query.post==="string"?query.post:typeof query.postId==="string"?query.postId:null;redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/${post?`posts/${encodeURIComponent(post)}`:"home"}`));}
