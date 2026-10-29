import {invoiceDrawerListSearch} from "@erpx/feature-finance/lib/invoice-drawer-list-search";
import {FinanceSourceDocumentBackdrop} from "@erpx/feature-finance/components/source-document-backdrop";
import Screen from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/finance/invoices/screen";
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@erpx/feature-records/lib/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["invoice"]??[defaultRecordSection("invoice")]).includes(p.recordSection))notFound();const source=await FinanceSourceDocumentBackdrop({holdingSlug:p.holdingSlug,kind:"invoice",id:p.id});if(source)return source;const query=invoiceDrawerListSearch(`/holdings/${p.holdingSlug}`,await searchParams),props={params:Promise.resolve({...p,...{}}),searchParams:Promise.resolve({...query,detailKind:"invoice",detailId:p.id})} as Parameters<typeof Screen>[0]; return (await Screen(props))??null;}
