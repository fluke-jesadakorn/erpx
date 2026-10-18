import Screen from "@/app/holdings/[holdingSlug]/_presenters/ecommerce/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["ecommerce_checkout"]??[defaultRecordSection("ecommerce_checkout")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"ecommerce_checkout",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
