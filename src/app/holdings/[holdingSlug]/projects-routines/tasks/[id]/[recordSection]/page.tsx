import {ProjectTaskSourceRecordBackdrop} from "@/components/projects/task-source-record-backdrop";
import Screen from "@/app/holdings/[holdingSlug]/_presenters/work/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["staff_task"]??[defaultRecordSection("staff_task")]).includes(p.recordSection))notFound();const source=await ProjectTaskSourceRecordBackdrop({holdingSlug:p.holdingSlug,id:p.id});if(source)return source;const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{"type":"staff_task","view":"all"},detailKind:"staff_task",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
