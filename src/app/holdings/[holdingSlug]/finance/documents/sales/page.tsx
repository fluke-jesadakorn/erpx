import {DestinationIndex} from '@/components/workplace/destination-index';
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}) {return <DestinationIndex appearance="standard" holdingSlug={(await params).holdingSlug} path="finance/documents/sales"/>;}
