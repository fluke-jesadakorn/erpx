"use client";
import type { ReactNode } from 'react';
import {useMoneyFormatter} from "@erpx/feature-finance/components/money/currency-provider";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import { RecordCollectionDrawer, RecordFacts } from '@erpx/feature-records/components/record-collection';
import type { PmsWorkspace } from "../../../../packages/server-pms/src/public/pms";
/** Console and Front each call their own owner API and keep its visibility projection. */
export function PmsEntryDetail({base,propertyId,renderAction,recovery}:{base:string;propertyId?:string;recovery?:ReactNode;renderAction?:(id:string,snapshot:PmsWorkspace)=>ReactNode}) {
 const formatMoney=useMoneyFormatter();
 const query=useSearchParams(),id=query.get('detailKind')==='pms_entry'?query.get('detailId'):null;
 const params=new URLSearchParams({view:'detail',id:id??'',...(propertyId?{property:propertyId}:{})});
 return <RecordCollectionDrawer<PmsWorkspace> kind="pms_entry" title="Guest Account entry" recovery={recovery} endpoint={`/api/v1${base}/pms?${params}`} select={(snapshot,entryId)=>{
  const row=snapshot.snapshot?.records.find(record=>record.id===entryId&&record.kind==='entry');
  return row?{id:row.id,version:row.version,...row.data}:null;
 }} render={(record,snapshot)=><><RecordFacts record={record} fields={['id','version','date','kind','description','reference','method','state','originalId']}/><dl className="ui-form-grid">{['net','service','tax','total'].filter(field=>record[field]!==undefined).map(field=><div key={field}><dt>{field}</dt><dd>{formatMoney(Number(record[field]),snapshot.snapshot?.property.config.currency??'THB',String(record.date),2)}</dd></div>)}</dl>{renderAction?.(String(record.id),snapshot)}</>}/>;
}
