import Screen from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/legal/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@erpx/feature-records/lib/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["legal"]??[defaultRecordSection("legal")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{"kind":"counsel"},detailKind:"legal",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
