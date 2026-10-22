import Screen from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/people/payroll/history/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@erpx/feature-records/lib/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["payroll_run"]??[defaultRecordSection("payroll_run")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"payroll_run",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
