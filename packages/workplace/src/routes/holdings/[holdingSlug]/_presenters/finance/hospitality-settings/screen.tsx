import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hospitalitySetup } from "@erpx/server-pms/public/hospitality-setup";
import { StandardHospitalitySetupGuide } from "@erpx/feature-shared/components/hospitality/standard-setup-guide";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
export const dynamic="force-dynamic";
export default async function HotelAccountingSettings({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params, query=await searchParams, actor=requirePermission(await requireWorkContext(holdingSlug),"finance.read"), base=workspaceBase(publicScope(actor));
  const data=await hospitalitySetup(actor,query.branch,query.property).catch(error=>pageError(error,holdingSlug));
  return <FinanceWorkspace base={base} active="hotels" title="Hotel accounting settings" description="Prepare and independently approve hotel posting accounts and payment liabilities.">
    <StandardHospitalitySetupGuide key={`${actor.companyId}:${actor.branchId}:${actor.assignmentId}`} owner={{userId:actor.userId,companyId:actor.companyId,branchId:actor.branchId}} canBootstrapPos={!actor.branchId&&hasPermission(actor,'pos.configure')&&hasPermission(actor,'pos.accounting.prepare')&&hasPermission(actor,'finance.write')} canReadPos={hasPermission(actor,'pos.read')} initial={data} base={base}/>
  </FinanceWorkspace>;
}
