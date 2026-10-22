import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { hospitalitySetup } from "@erpx/server-pms/public/hospitality-setup";
import { StandardHospitalitySetupGuide } from "@erpx/feature-shared/components/hospitality/standard-setup-guide";
import { pageError } from "@erpx/server-work_projection/public/page-error";
export const dynamic='force-dynamic';
export default async function StandardHospitalitySetupPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
  let data;
  try {data=await hospitalitySetup(actor,query.branch,query.property);}
  catch(error) {return pageError(error,holdingSlug);}
  return <StandardHospitalitySetupGuide key={`${data.companyId}:${data.branchId}`} owner={{userId:actor.userId,companyId:actor.companyId,branchId:actor.branchId}} canBootstrapPos={!actor.branchId&&hasPermission(actor,'pos.configure')&&hasPermission(actor,'pos.accounting.prepare')&&hasPermission(actor,'finance.write')} canReadPos={hasPermission(actor,'pos.read')} initial={data} base={canonicalUiHref(`/holdings/${holdingSlug}`)}/>;
}
