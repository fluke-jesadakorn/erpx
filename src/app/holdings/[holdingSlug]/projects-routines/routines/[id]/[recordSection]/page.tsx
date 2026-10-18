import Screen from "@/app/holdings/[holdingSlug]/_presenters/routine-templates/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["routine_template"]??[defaultRecordSection("routine_template")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"routine_template",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
