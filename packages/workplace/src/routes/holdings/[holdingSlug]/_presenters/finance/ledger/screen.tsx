import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { redirect } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";

export default async function LegacyLedger({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{chart?:string;status?:string;cursor?:string;new?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),query=await searchParams;
  const search=new URLSearchParams(Object.entries(query).flatMap(([key,value])=>value?[[key,value]]:[]));
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/finance/accounting${search.size?`?${search}`:""}`));
}
