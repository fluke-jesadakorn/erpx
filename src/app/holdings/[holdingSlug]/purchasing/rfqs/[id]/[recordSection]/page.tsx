import Screen from "@/app/holdings/[holdingSlug]/_presenters/procurement-rfqs/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["rfq"]??[defaultRecordSection("rfq")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"rfq",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
