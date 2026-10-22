import {DestinationIndex} from '@erpx/workplace/components/destination-index';
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@erpx/feature-records/lib/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["marketing_campaign_sources"]??[defaultRecordSection("marketing_campaign_sources")]).includes(p.recordSection))notFound();return <DestinationIndex appearance="standard" holdingSlug={p.holdingSlug} path="customers-sales/marketing"/>;}
