import {redirect} from "next/navigation";
import {canonicalUiHref} from "@erpx/work-projection-ui/lib/destination-registry";
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const query=new URLSearchParams();for(const [key,value] of Object.entries(await searchParams))if(!["view","section"].includes(key))for(const item of Array.isArray(value)?value:value?[value]:[])query.append(key,item);
 redirect(canonicalUiHref(`/holdings/${encodeURIComponent((await params).holdingSlug)}/people?view=external&section=teams&${query}`));
}
