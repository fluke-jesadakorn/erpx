import {DestinationIndex} from '@/components/workplace/destination-index';
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p={...await params,recordSection:"overview"};if(!(recordSections["pms_counter_refund"]??[defaultRecordSection("pms_counter_refund")]).includes(p.recordSection))notFound();return <DestinationIndex appearance="legacy" holdingSlug={p.holdingSlug} path="pms"/>;}
