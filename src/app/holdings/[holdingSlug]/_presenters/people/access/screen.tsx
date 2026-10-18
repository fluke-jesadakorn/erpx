import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { redirect } from "next/navigation";
export default async function PeopleAccessPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<{view?:string}>}){
 const {holdingSlug}=await params,{view}=await searchParams;
 redirect(canonicalUiHref(`/holdings/${encodeURIComponent(holdingSlug)}/technology?view=${['groups','policies'].includes(view??'')?'directory':view==='devices'?'resources':'resources'}`));
}
