import {QualitySourceRecordBackdrop} from "@erpx/quality-ui/components/source-record-backdrop";
import Screen from "@erpx/platform-workplace/routes/holdings/[holdingSlug]/_presenters/quality/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@erpx/records-ui/lib/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["quality_hold"]??[defaultRecordSection("quality_hold")]).includes(p.recordSection))notFound();const source=await QualitySourceRecordBackdrop({holdingSlug:p.holdingSlug,kind:"hold",id:p.id});if(source)return source;const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"quality_hold",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
