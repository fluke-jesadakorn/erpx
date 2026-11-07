import {headers} from 'next/headers';
import {notFound,redirect} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@erpx/feature-records/lib/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {
 const p={...await params,recordSection:"overview"};
 if(!(recordSections["pos_sale"]??[defaultRecordSection("pos_sale")]).includes(p.recordSection))notFound();
 const search=await searchParams;
 if(search.detailKind!=="pos_sale"||search.detailId!==p.id){
  const path=(await headers()).get("x-erpx-page-path")??"";
  const query=new URLSearchParams({tab:"history",detailKind:"pos_sale",detailId:p.id});
  redirect(`${path}/overview?${query}`);
 }
 const Screen=(await import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/pos/standard-entry")).default;
 const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...search,...{"tab":"history","queue":"approval"},detailKind:"pos_sale",detailId:p.id})} as Parameters<typeof Screen>[0];
 return (await Screen(props))??null;
}
