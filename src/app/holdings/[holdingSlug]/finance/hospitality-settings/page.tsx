import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { workspaceBase } from "@/lib/workspace";
import { hospitalitySetup } from "@/server/modules/pms/public/hospitality-setup";
import { HospitalitySetupGuide } from "@/components/hospitality/setup-guide";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function HotelAccountingSettings({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params, query=await searchParams, actor=requirePermission(await requireWorkContext(holdingSlug),"finance.read"), base=workspaceBase(publicScope(actor));
  const data=await hospitalitySetup(actor,query.branch,query.property).catch(error=>pageError(error,holdingSlug));
  return <FinanceWorkspace base={base} active="hotels" title="Hotel accounting settings" description="Prepare and independently approve hotel posting accounts and payment liabilities.">
    <HospitalitySetupGuide key={`${actor.companyId}:${actor.branchId}:${actor.assignmentId}`} initial={data} base={base}/>
  </FinanceWorkspace>;
}
