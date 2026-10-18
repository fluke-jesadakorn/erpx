import {DestinationIndex} from '@/components/workplace/destination-index';
import {notFound} from 'next/navigation';
import {recordSections,defaultRecordSection} from '@/lib/records/record-drawer';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string;id:string;recordSection?:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {const p={...await params,recordSection:"overview"};if(!(recordSections["brand_version"]??[defaultRecordSection("brand_version")]).includes(p.recordSection))notFound();return <DestinationIndex appearance="standard" holdingSlug={p.holdingSlug} path="administration/brand"/>;}
