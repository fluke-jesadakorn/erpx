import { hasPermission } from '@/lib/identity_access/permissions';
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { hospitalitySetup } from "@/server/modules/pms/public/hospitality-setup";
import { StandardHospitalitySetupGuide } from "@/components/shared/hospitality/standard-setup-guide";
import { pageError } from "@/server/modules/work_projection/public/page-error";
export const dynamic='force-dynamic';
export default async function StandardHospitalitySetupPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
  let data;
  try {data=await hospitalitySetup(actor,query.branch,query.property);}
  catch(error) {return pageError(error,holdingSlug);}
  return <StandardHospitalitySetupGuide key={`${data.companyId}:${data.branchId}`} owner={{userId:actor.userId,companyId:actor.companyId,branchId:actor.branchId}} canBootstrapPos={!actor.branchId&&hasPermission(actor,'pos.configure')&&hasPermission(actor,'pos.accounting.prepare')&&hasPermission(actor,'finance.write')} canReadPos={hasPermission(actor,'pos.read')} initial={data} base={canonicalUiHref(`/holdings/${holdingSlug}`)}/>;
}
