import 'server-only';
import {z} from 'zod';
import {dateField,dates,plusDays,type Config} from '@/lib/pms';
import {requirePermission} from '../iam-permissions';
import {ApiError} from '../http';
import type {WorkContext} from '../authorization';
import type {SqlDatabase} from '../runtime/types';
export const pmsReportRange=z.object({propertyId:z.string().min(1),from:dateField,to:dateField}).refine(v=>v.from<=v.to&&Date.parse(v.to)-Date.parse(v.from)<93*86400000,{message:'Choose up to 93 days'});
async function scope(db:SqlDatabase,actor:WorkContext,propertyId:string){
 requirePermission(actor,'pms.read');requirePermission(actor,'pms.reports');
 const row=await db.prepare('SELECT branch_id,config_json FROM pms_properties WHERE id=? AND company_id=?').bind(propertyId,actor.companyId).first<{branch_id:string;config_json:string}>();
 if(!row||actor.departmentId||actor.beta||actor.branchId&&actor.branchId!==row.branch_id)throw new ApiError(404,'PMS_NOT_FOUND','Hotel not found in this workspace');
 return JSON.parse(row.config_json) as Config;
}
export async function pmsRangeReport(db:SqlDatabase,actor:WorkContext,raw:unknown){
 const {propertyId,from,to}=pmsReportRange.parse(raw),config=await scope(db,actor,propertyId);
 const rows=(await db.prepare(`WITH RECURSIVE days AS (SELECT generate_series(?::date,?::date,interval '1 day')::date d),
 lineage AS (
  SELECT id root,id,data_json::jsonb data,ARRAY[id] seen FROM pms_records WHERE property_id=? AND kind='entry'
   AND data_json::jsonb->>'date'>=? AND data_json::jsonb->>'date'<=?
  UNION ALL SELECT l.root,e.id,e.data_json::jsonb,l.seen||e.id FROM lineage l JOIN pms_records e
   ON e.id=l.data->>'originalId' AND e.property_id=? AND e.kind='entry' WHERE NOT e.id=ANY(l.seen)
 ), room_entries AS (SELECT DISTINCT root FROM lineage WHERE data->>'sourceKey' LIKE 'night:%')
 SELECT d::text date,
 COALESCE((SELECT (data_json::jsonb->'roomMetrics'->>'capacity')::bigint FROM pms_records WHERE property_id=? AND kind='night' AND data_json::jsonb->>'date'=d::text AND data_json::jsonb->>'state'='completed' LIMIT 1),
  (SELECT COUNT(*) FROM pms_records r WHERE r.property_id=? AND r.kind='room' AND r.data_json::jsonb->>'active'='true' AND NOT EXISTS(SELECT 1 FROM pms_records b WHERE b.property_id=r.property_id AND b.kind='block' AND b.data_json::jsonb->>'kind'='maintenance' AND b.data_json::jsonb->>'state'='active' AND b.data_json::jsonb->>'roomId'=r.id AND b.data_json::jsonb->>'from'<=d::text))) capacity,
 COALESCE((SELECT (data_json::jsonb->'roomMetrics'->>'sold')::bigint FROM pms_records WHERE property_id=? AND kind='night' AND data_json::jsonb->>'date'=d::text AND data_json::jsonb->>'state'='completed' LIMIT 1),
  (SELECT COUNT(*) FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state' IN ('in_house','checked_out') AND data_json::jsonb->>'arrival'<=d::text AND data_json::jsonb->>'departure'>d::text)) sold,
 COALESCE(SUM((e.data_json::jsonb->>'net')::bigint) FILTER(WHERE e.id IN (SELECT root FROM room_entries)),0) room_revenue,
 COALESCE(SUM((e.data_json::jsonb->>'net')::bigint+(e.data_json::jsonb->>'service')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) net_revenue,
 COALESCE(SUM((e.data_json::jsonb->>'tax')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind' IN ('charge','adjustment')),0) tax,
 COALESCE(-SUM((e.data_json::jsonb->>'total')::bigint) FILTER(WHERE e.data_json::jsonb->>'kind'='refund'),0) refunds,
 (SELECT COUNT(*) FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state'='cancelled' AND to_char(to_timestamp(updated_at/1000.0) AT TIME ZONE ?,'YYYY-MM-DD')=d::text) cancellations,
 (SELECT COUNT(*) FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'state'='no_show' AND data_json::jsonb->>'arrival'=d::text) no_shows
 FROM days LEFT JOIN pms_records e ON e.property_id=? AND e.kind='entry' AND e.data_json::jsonb->>'date'=d::text GROUP BY d ORDER BY d`)
 .bind(from,to,propertyId,from,to,propertyId,propertyId,propertyId,propertyId,propertyId,propertyId,config.timezone,propertyId,propertyId)
 .all<{date:string;capacity:number;sold:number;room_revenue:number;net_revenue:number;tax:number;refunds:number;cancellations:number;no_shows:number}>()).results;
 const balances=(await db.prepare(`SELECT l.account_id,SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
 JOIN (SELECT property_id,journal_id FROM pms_postings UNION SELECT property_id,opening_journal_id journal_id FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL) p ON p.journal_id=j.id
 WHERE p.property_id=? AND j.status='posted' AND j.entry_date<=? GROUP BY l.account_id`).bind(propertyId,to).all<{account_id:string;balance:number}>()).results;
 const balance=(account:string|undefined,sign=1)=>balances.filter(r=>r.account_id===account).reduce((n,r)=>n+Number(r.balance)*sign,0);
 const totals=rows.reduce((n,r)=>({capacity:n.capacity+Number(r.capacity),sold:n.sold+Number(r.sold),roomRevenue:n.roomRevenue+Number(r.room_revenue),netRevenue:n.netRevenue+Number(r.net_revenue),tax:n.tax+Number(r.tax),refunds:n.refunds+Number(r.refunds),cancellations:n.cancellations+Number(r.cancellations),noShows:n.noShows+Number(r.no_shows)}),{capacity:0,sold:0,roomRevenue:0,netRevenue:0,tax:0,refunds:0,cancellations:0,noShows:0});
 return {from,to,currency:config.currency,days:rows,totals,occupancy:totals.capacity?totals.sold/totals.capacity:0,adr:totals.sold?Math.round(totals.roomRevenue/totals.sold):0,revpar:totals.capacity?Math.round(totals.roomRevenue/totals.capacity):0,balances:{receivable:balance(config.accounts.receivable),advance:balance(config.accounts.advance,-1),deposit:balance(config.accounts.deposit,-1),refundPayable:balance(config.onlineAccounting?.refundPayable,-1),unallocated:balance(config.onlineAccounting?.unallocated,-1)}};
}
export async function pmsReportEntries(db:SqlDatabase,actor:WorkContext,propertyId:string,date:string,page:number){
 dateField.parse(date);z.number().int().min(1).max(10000).parse(page);await scope(db,actor,propertyId);
 const rows=(await db.prepare("SELECT id,data_json FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'date'=? ORDER BY updated_at,id LIMIT 51 OFFSET ?").bind(propertyId,date,(page-1)*50).all<{id:string;data_json:string}>()).results;
 return {rows:rows.slice(0,50).map(r=>({id:r.id,...JSON.parse(r.data_json)})),hasNext:rows.length>50,page};
}
// Kept beside the range schema for clients to choose an inclusive default week.
export const reportDates=(from:string,to:string)=>dates(from,plusDays(to,1));
