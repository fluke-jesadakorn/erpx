import { redirect } from "next/navigation";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { peopleContextHref } from "@erpx/people-payroll-ui/lib/people-route-context";
export const dynamic="force-dynamic";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),search=await searchParams;
 redirect(peopleContextHref(workspaceBase(publicScope(actor)),search));
}
