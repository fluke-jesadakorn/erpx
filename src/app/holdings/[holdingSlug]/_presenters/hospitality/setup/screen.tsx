import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { hospitalitySetup } from "@/server/modules/pms/public/hospitality-setup";
import { HospitalitySetupGuide } from "@/components/shared/hospitality/setup-guide";
import { pageError } from "@/server/modules/work_projection/public/page-error";
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
  let data;
  try {data=await hospitalitySetup(actor,query.branch,query.property);}
  catch(error) {return pageError(error,holdingSlug);}
  return <HospitalitySetupGuide key={`${data.companyId}:${data.branchId}`} initial={data} base={canonicalUiHref(`/holdings/${holdingSlug}`)}/>;
}
