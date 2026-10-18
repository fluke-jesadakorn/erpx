import Owner from '@/app/holdings/[holdingSlug]/_presenters/work/routing/[kind]/[id]/screen';
import {RecordDetailBoundary} from '@/components/records/record-detail-boundary';
import {notFound} from 'next/navigation';
export default async function Page({params}:{params:Promise<{holdingSlug:string;kind:string;id:string;recordSection:string}>}){const p=await params;if(p.recordSection!=='overview')notFound();return <RecordDetailBoundary holdingSlug={p.holdingSlug} target={{kind:'work_flow',id:p.id,args:{kind:p.kind,view:'routing'}}}>{await Owner({params:Promise.resolve(p)} )}</RecordDetailBoundary>;}
