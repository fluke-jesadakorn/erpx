import Owner from '@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/work/department-approvals/[kind]/[id]/screen';
import {RecordDetailBoundary} from '@erpx/feature-records/components/record-detail-boundary';
import {notFound} from 'next/navigation';
export default async function Page({params}:{params:Promise<{holdingSlug:string;kind:string;id:string;recordSection:string}>}){const p=await params;if(p.recordSection!=='overview')notFound();return <RecordDetailBoundary holdingSlug={p.holdingSlug} target={{kind:'work_flow',id:p.id,args:{kind:p.kind,view:'department-approvals'}}}><Owner params={Promise.resolve(p)} searchParams={Promise.resolve({})}/></RecordDetailBoundary>;}
