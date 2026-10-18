import {DestinationIndex} from '@/components/workplace/destination-index';
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["finance_report"]??[defaultRecordSection("finance_report")]).includes(p.recordSection))notFound();return <DestinationIndex appearance="standard" holdingSlug={p.holdingSlug} path="finance/accounting"/>;}
