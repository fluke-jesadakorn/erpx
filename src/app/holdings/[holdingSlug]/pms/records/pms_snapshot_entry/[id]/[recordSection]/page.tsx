import {DestinationIndex} from '@/components/workplace/destination-index';
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p=await params;if(!(recordSections["pms_snapshot_entry"]??[defaultRecordSection("pms_snapshot_entry")]).includes(p.recordSection))notFound();return <DestinationIndex appearance="legacy" holdingSlug={p.holdingSlug} path="pms"/>;}
