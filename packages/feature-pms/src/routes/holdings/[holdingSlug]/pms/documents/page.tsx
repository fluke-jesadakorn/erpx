import {DestinationIndex} from '@erpx/workplace/components/destination-index';
export default async function Page({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string|string[]|undefined>>}) {return <DestinationIndex openWork searchParams={searchParams} appearance="legacy" holdingSlug={(await params).holdingSlug} path="pms/documents"/>;}
