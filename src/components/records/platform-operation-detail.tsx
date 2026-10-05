"use client";
import {RecordCollectionDrawer,RecordFacts,CollectionRecordLink} from './record-collection';
import type {PlatformOperationsSummary} from '@/server/platform-operations';
export function PlatformOperationDetail(){return <RecordCollectionDrawer<PlatformOperationsSummary> kind="platform_operation" title="Holding operations" endpoint="/api/v1/platform/operations" select={(snapshot,id)=>snapshot.holdings.find(row=>row.id===id) as unknown as Record<string,unknown>??null} render={(record,snapshot)=>{
 const row=snapshot.holdings.find(row=>row.id===record.id)!;
 return <><RecordFacts record={record} fields={['displayName','slug','status','pendingEvents','deadLetterEvents','claimedEvents','oldestPendingAt','jobHeartbeatAt','error']}/><h2>Migration status</h2><RecordFacts record={row.migrations}/><h2>Dead letters</h2>{row.deadLetters.map(event=><article key={event.id}><RecordFacts record={event} fields={['id','eventType','aggregateId','aggregateVersion','attempts','lastError','deadLetterAt']}/></article>)}</>;
 }}/>;}
export {CollectionRecordLink};
