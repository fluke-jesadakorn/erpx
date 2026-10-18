import {QualitySourceRecordBackdrop} from "@/components/quality/source-record-backdrop";
import Screen from "@/app/holdings/[holdingSlug]/_presenters/stock-counts/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p={...await params,recordSection:"overview"};if(!(recordSections["stock_count"]??[defaultRecordSection("stock_count")]).includes(p.recordSection))notFound();const source=await QualitySourceRecordBackdrop({holdingSlug:p.holdingSlug,kind:"count",id:p.id});if(source)return source;const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"stock_count",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
