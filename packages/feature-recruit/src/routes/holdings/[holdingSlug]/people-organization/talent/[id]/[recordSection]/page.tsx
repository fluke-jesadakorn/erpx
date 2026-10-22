import Screen from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/talent/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@erpx/feature-records/lib/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["talent_application"]??[defaultRecordSection("talent_application")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"talent_application",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
