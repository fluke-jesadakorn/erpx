import {FinanceSourceDocumentBackdrop} from "@/components/finance/source-document-backdrop";
import Screen from "@/app/holdings/[holdingSlug]/_presenters/finance/invoices/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p={...await params,recordSection:"overview"};if(!(recordSections["invoice"]??[defaultRecordSection("invoice")]).includes(p.recordSection))notFound();const source=await FinanceSourceDocumentBackdrop({holdingSlug:p.holdingSlug,kind:"invoice",id:p.id});if(source)return source;const props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...await searchParams,...{},detailKind:"invoice",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
