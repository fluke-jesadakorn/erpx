import { requireWorkContext } from '@/server/authorization';
import { hospitalitySetup } from '@/server/hospitality-setup';
import { HospitalitySetupGuide } from '@/components/hospitality/setup-guide';
import { pageError } from '@/server/page-error';
export const dynamic='force-dynamic';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{branch?:string;property?:string}>}) {
  const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),query=await searchParams;
  let data;
  try {data=await hospitalitySetup(actor,query.branch,query.property);}
  catch(error) {return pageError(error,holdingSlug);}
  return <HospitalitySetupGuide key={`${data.companyId}:${data.branchId}`} initial={data} base={`/holdings/${holdingSlug}`}/>;
}
