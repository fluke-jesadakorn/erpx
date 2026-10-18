import Screen from "@/app/holdings/[holdingSlug]/_presenters/stock-transfers/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p={...await params,recordSection:"overview"};if(!(recordSections["stock_transfer"]??[defaultRecordSection("stock_transfer")]).includes(p.recordSection))notFound();const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"stock_transfer",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
