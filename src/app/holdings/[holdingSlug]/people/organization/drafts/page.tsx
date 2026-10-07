import { redirect } from "next/navigation";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { peopleContextHref } from "@/lib/people-route-context";
export const dynamic="force-dynamic";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),search=await searchParams;
 redirect(peopleContextHref(workspaceBase(publicScope(actor)),search));
}
