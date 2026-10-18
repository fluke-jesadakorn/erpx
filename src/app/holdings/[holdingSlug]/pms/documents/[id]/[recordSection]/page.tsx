import Screen from "@/app/holdings/[holdingSlug]/_presenters/pms/standard-entry";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["pms_document"]??[defaultRecordSection("pms_document")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{section:['reports']}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"pms_document",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
