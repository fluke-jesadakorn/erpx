import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { hospitalitySetup } from "@erpx/server-pms/public/hospitality-setup";
import { HospitalitySetupGuide } from "@erpx/feature-shared/components/hospitality/setup-guide";
import { pageError } from "@erpx/server-work_projection/public/page-error";
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
  let data;
  try {data=await hospitalitySetup(actor,query.branch,query.property);}
  catch(error) {return pageError(error,holdingSlug);}
  return <HospitalitySetupGuide key={`${data.companyId}:${data.branchId}`} initial={data} base={canonicalUiHref(`/holdings/${holdingSlug}`)}/>;
}
