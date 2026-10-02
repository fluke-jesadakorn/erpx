import { redirect } from "next/navigation";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";

export default async function LegacyLedger({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{chart?:string;status?:string;cursor?:string;new?:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),query=await searchParams;
  const search=new URLSearchParams(Object.entries(query).flatMap(([key,value])=>value?[[key,value]]:[]));
  redirect(`${workspaceBase(publicScope(actor))}/finance/accounting${search.size?`?${search}`:""}`);
}
