import {DestinationIndex} from '@erpx/workplace/components/destination-index';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}) {return <DestinationIndex appearance="standard" holdingSlug={(await params).holdingSlug} path="tasks"/>;}
