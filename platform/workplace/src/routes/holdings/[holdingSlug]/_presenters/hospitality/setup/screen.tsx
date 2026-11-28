import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { hospitalitySetup } from "@erpx/pms-server/public/hospitality-setup";
import { HospitalitySetupGuide } from "@erpx/platform-shared-ui/components/hospitality/setup-guide";
import { pageError } from "@erpx/work-projection-server/public/page-error";
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
  let data;
  try {data=await hospitalitySetup(actor,query.branch,query.property);}
  catch(error) {return pageError(error,holdingSlug);}
  return <HospitalitySetupGuide key={`${data.companyId}:${data.branchId}`} initial={data} base={canonicalUiHref(`/holdings/${holdingSlug}`)}/>;
}
