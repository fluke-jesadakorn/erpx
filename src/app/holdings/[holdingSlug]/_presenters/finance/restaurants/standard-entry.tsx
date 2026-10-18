import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { posWorkspace } from "@/server/modules/pos/public/pos";
import { StandardBackofficePosConsole as PosConsole } from "@/components/pos/standard-pos-backoffice-console";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function RestaurantAccounting({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{tab?:string;sale?:string}>}) {
  const {holdingSlug}=await params, query=await searchParams, actor=await requireWorkContext(holdingSlug), base=workspaceBase(publicScope(actor));
  const tab=query.tab==="settings"?"settings":"reports";
  const data=await posWorkspace(actor,query.sale,"",tab,undefined,false,true).catch(error=>pageError(error,holdingSlug));
  return <FinanceWorkspace base={base} active="restaurants" title="Restaurant accounting" description="Prepare and independently approve posting accounts, verify transfers, reconcile settlement and confirm daily accounting.">
    <PosConsole key={`${actor.companyId}:${actor.branchId}:${actor.assignmentId}:${tab}`} initial={data} base={base} initialTab={tab} initialRead={{sale:query.sale,tab,finance:true}} financeMode/>
  </FinanceWorkspace>;
}
