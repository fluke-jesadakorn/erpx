import "server-only";
import {plusDays,type Kind,type PmsRecord} from "@/lib/pms-model";
import type {PmsReadSelection} from '@/lib/pms-read-selection';
import type { PmsSetupFacts } from "@/lib/pms-read";
import type { SqlDatabase } from "../runtime/types";

type RecordRow = { id: string; kind: Kind; data_json: string; version: number; updated_at: number };
async function readRecords(db: SqlDatabase, sql: string, args: unknown[]) {
  const rows = (await db.prepare(sql).bind(...args).all<RecordRow>()).results;
  return rows.map(row => ({ id: row.id, kind: row.kind, data: JSON.parse(row.data_json),
    version: Number(row.version), updatedAt: Number(row.updated_at) })) as PmsRecord[];
}

export function readAllPmsRecords(db: SqlDatabase, propertyId: string) {
  return readRecords(db, "SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? ORDER BY updated_at,id", [propertyId]);
}

export function readPmsConfiguration(db:SqlDatabase,propertyId:string){
 return readRecords(db,`SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND (
   kind IN ('room_type','room','rate') OR (kind='reservation' AND id IN (
     SELECT reservation_id FROM pms_booking_emails WHERE property_id=? AND state!='sent' ORDER BY created_at DESC LIMIT 50
   ))) ORDER BY updated_at,id`,[propertyId,propertyId]);
}

export async function readPmsOperationsList(db:SqlDatabase,propertyId:string,businessDate:string,selection:Extract<PmsReadSelection,{view:'list'}>){
 if(selection.section!=='reservations'&&selection.section!=='front-desk')return readPmsSectionList(db,propertyId,businessDate,selection);
 const from=selection.from??plusDays(businessDate,-30),to=selection.to??plusDays(from,90),kind=selection.section==='reservations'?'reservation':'stay';
 if(from>to||Date.parse(to)-Date.parse(from)>93*86400000)throw new Error('PMS_LIST_RANGE_INVALID');
 const selected=(await db.prepare(`SELECT r.id FROM pms_records r
   LEFT JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' AND g.property_id=r.property_id AND g.kind='guest'
   WHERE r.property_id=? AND r.kind=?
   AND (r.data_json::jsonb->>'state'=? OR ?='' OR (?='arrivals' AND r.data_json::jsonb->>'state'='reserved' AND r.data_json::jsonb->>'arrival'=?) OR (?='departures' AND r.data_json::jsonb->>'state'='in_house' AND r.data_json::jsonb->>'departure'<=?))
   AND (?<>'' OR r.kind='reservation' OR r.data_json::jsonb->>'state' IN ('reserved','in_house'))
   AND (CASE WHEN r.kind='stay' THEN (r.data_json::jsonb->>'arrival'<=? AND r.data_json::jsonb->>'departure'>=?) OR r.data_json::jsonb->>'state'='in_house'
     ELSE EXISTS(SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'reservationId'=r.id AND ((st.data_json::jsonb->>'arrival'<=? AND st.data_json::jsonb->>'departure'>=?) OR st.data_json::jsonb->>'state'='in_house')) END)
   AND (?='' OR position(lower(?) in lower(concat_ws(' ',g.data_json::jsonb->>'name',r.id,r.data_json::jsonb->>'number',(SELECT data_json::jsonb->>'number' FROM pms_records parent WHERE parent.property_id=r.property_id AND parent.id=r.data_json::jsonb->>'reservationId'))))>0)
   ORDER BY COALESCE(r.data_json::jsonb->>'arrival',r.updated_at::text),r.id LIMIT 51 OFFSET ?`).bind(propertyId,kind,selection.status,selection.status,selection.status,businessDate,selection.status,businessDate,selection.status,to,from,to,from,selection.q,selection.q,selection.page*50).all<{id:string}>()).results;
 const ids=selected.slice(0,50).map(r=>r.id),seeds=[...ids,...(selection.selectedId?[selection.selectedId]:[])];
 const records=await readRecords(db,`WITH chosen AS (SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))),
   reservations AS (SELECT id FROM chosen WHERE kind='reservation' UNION SELECT data->>'reservationId' FROM chosen),
   stays AS (SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay' AND (id IN (SELECT id FROM chosen) OR data_json::jsonb->>'reservationId' IN (SELECT id FROM reservations))),
   folios AS (SELECT data->>'folioId' id FROM stays)
   SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND (
    kind IN ('room_type','room','rate') OR id IN (SELECT id FROM chosen) OR id IN (SELECT id FROM reservations) OR id IN (SELECT id FROM stays) OR id IN (SELECT id FROM folios)
    OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))
    OR (kind='entry' AND data_json::jsonb->>'folioId' IN (SELECT id FROM folios))) ORDER BY updated_at,id`,[propertyId,JSON.stringify(seeds),propertyId,propertyId]);
 return {records,listMeta:{section:selection.section,kind,ids,from,to,page:selection.page,q:selection.q,status:selection.status,hasNext:selected.length>50,selectedId:selection.selectedId}};
}

/** Primary records are paged; related facts never expand into the hotel's history. */
async function readPmsSectionList(db:SqlDatabase,propertyId:string,businessDate:string,selection:Extract<PmsReadSelection,{view:'list'}>){
 const section=selection.section;
 const kind:Kind=section==='rooms'||section==='housekeeping'?'room':section==='folios'?'folio':section==='cashier'?'shift':section==='inventory'?'item':section==='guests'?'guest':'night';
 const from=selection.from??plusDays(businessDate,-30),to=selection.to??plusDays(from,90);
 if(from>to||Date.parse(to)-Date.parse(from)>93*86400000)throw new Error('PMS_LIST_RANGE_INVALID');
 const selected=(await db.prepare(`SELECT id FROM pms_records WHERE property_id=? AND kind=?
   AND (?='' OR COALESCE(data_json::jsonb->>'cleanliness',data_json::jsonb->>'state')=?)
   AND (?='' OR position(lower(?) in lower(concat_ws(' ',id,data_json::jsonb->>'name',data_json::jsonb->>'number',data_json::jsonb->>'phone',data_json::jsonb->>'email',data_json::jsonb->>'building',data_json::jsonb->>'floor')))>0)
   AND (kind NOT IN ('shift','night') OR data_json::jsonb->>'date' BETWEEN ? AND ? OR data_json::jsonb->>'state' IN ('open','running','blocked'))
   ORDER BY CASE WHEN kind='room' THEN data_json::jsonb->>'number' WHEN kind='guest' THEN data_json::jsonb->>'name' ELSE '' END,updated_at DESC,id LIMIT 51 OFFSET ?`)
   .bind(propertyId,kind,selection.status,selection.status,selection.q,selection.q,from,to,selection.page*50).all<{id:string}>()).results;
 const ids=selected.slice(0,50).map(row=>row.id),seeds=[...ids,...(selection.selectedId?[selection.selectedId]:[])];
 const roomView=section==='rooms'||section==='housekeeping',nightView=section==='night-audit',inventoryView=section==='inventory';
 const records=await readRecords(db,`WITH chosen AS (
   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))
 ), guest_history AS (
   SELECT id,data_json::jsonb data,row_number() OVER(PARTITION BY data_json::jsonb->>'guestId' ORDER BY data_json::jsonb->>'arrival' DESC,id) position
   FROM pms_records WHERE property_id=? AND kind='stay' AND data_json::jsonb->>'guestId' IN (SELECT id FROM chosen WHERE kind='guest')
 ), stays AS (
   SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay' AND (
     id IN (SELECT id FROM chosen) OR data_json::jsonb->>'folioId' IN (SELECT id FROM chosen WHERE kind='folio')
     OR (data_json::jsonb->>'state'='in_house' AND ((? AND data_json::jsonb->>'roomId' IN (SELECT id FROM chosen WHERE kind='room')) OR ?))
     OR (? AND ((data_json::jsonb->>'state'='reserved' AND data_json::jsonb->>'arrival'<=?) OR (data_json::jsonb->>'state'='in_house')))
   ) UNION SELECT id,data FROM guest_history WHERE position<=20
 ), folios AS (SELECT id FROM chosen WHERE kind='folio' UNION SELECT data->>'folioId' FROM stays UNION SELECT data->>'folioId' FROM chosen),
 room_ids AS (SELECT id FROM chosen WHERE kind='room' UNION SELECT data->>'roomId' FROM stays UNION SELECT data->>'roomId' FROM chosen),
 recent_tasks AS (
   SELECT id FROM pms_records WHERE property_id=? AND ? AND kind='task'
     AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND data_json::jsonb->>'state' IN ('done','superseded')
     ORDER BY updated_at DESC,id LIMIT 50
 ),
 inventory_recent AS (
   SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND ? AND (
     (kind='lost' AND (data_json::jsonb->>'state'='stored' OR data_json::jsonb->>'foundDate' BETWEEN ? AND ?))
     OR (kind='movement' AND data_json::jsonb->>'date' BETWEEN ? AND ?)
   ) ORDER BY updated_at DESC,id LIMIT 50
 ) SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND (
   id IN (SELECT id FROM chosen) OR id IN (SELECT id FROM stays)
   OR (kind='reservation' AND id IN (SELECT data->>'reservationId' FROM stays))
   OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))
   OR (kind='room' AND (? OR id IN (SELECT id FROM room_ids)))
   OR (kind='guest' AND ? AND id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='guest' ORDER BY updated_at DESC,id LIMIT 50))
   OR (kind IN ('room_type','rate') AND (? OR ? OR id IN (SELECT data->>'typeId' FROM stays UNION SELECT data->>'rateId' FROM stays)))
   OR (kind='folio' AND id IN (SELECT id FROM folios))
   OR (kind IN ('entry','refund','correction') AND data_json::jsonb->>'folioId' IN (SELECT id FROM folios))
   OR (kind='task' AND ? AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND (data_json::jsonb->>'state' NOT IN ('done','superseded') OR id IN (SELECT id FROM recent_tasks)))
   OR (kind='block' AND ? AND data_json::jsonb->>'roomId' IN (SELECT id FROM room_ids) AND data_json::jsonb->>'state'='active')
   OR (kind='shift' AND (? OR ?) AND (data_json::jsonb->>'state'='open' OR (? AND data_json::jsonb->>'state'='closed' AND (data_json::jsonb->>'variance')::bigint<>0)))
   OR (kind='item' AND id IN (SELECT data->>'itemId' FROM inventory_recent WHERE kind='movement'))
   OR id IN (SELECT id FROM inventory_recent)
 ) ORDER BY updated_at,id`,[propertyId,JSON.stringify(seeds),propertyId,propertyId,roomView,inventoryView,nightView,businessDate,propertyId,roomView,propertyId,inventoryView,from,to,from,to,propertyId,inventoryView,inventoryView,propertyId,roomView,nightView,roomView,roomView,section==='folios',nightView,nightView]);
 return {records,listMeta:{section,kind,ids,from,to,page:selection.page,q:selection.q,status:selection.status,hasNext:selected.length>50,selectedId:selection.selectedId}};
}

/** Setup readiness describes the property, independently of a bounded view. */
export async function readPmsSetupFacts(db: SqlDatabase, propertyId: string): Promise<PmsSetupFacts> {
  return (await db.prepare(`SELECT
    EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='room_type') AS "hasRoomTypes",
    EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='room' AND data_json::jsonb->>'active'='true') AS "hasActiveRooms",
    EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='rate' AND data_json::jsonb->>'active'='true') AS "hasActiveRates"`)
    .bind(propertyId, propertyId, propertyId).first<PmsSetupFacts>())!;
}

// These readers are query models only. Commands must always load the complete,
// authoritative property state while holding its transaction lock.
const stayPredicate = `(data_json::jsonb->>'state' IN ('reserved','in_house') OR
    data_json::jsonb->>'arrival'=? OR data_json::jsonb->>'departure'=?)`;
export const pmsDashboardRecordsSql = `SELECT id,kind,data_json,version,updated_at FROM pms_records
    WHERE property_id=? AND (
      kind IN ('room_type','room','rate')
      OR (kind='stay' AND ${stayPredicate})
      OR (kind='task' AND data_json::jsonb->>'state' NOT IN ('done','superseded'))
      OR (kind='folio' AND data_json::jsonb->>'state'!='closed' AND (data_json::jsonb->>'due')::bigint>0)
      OR (kind='guest' AND id IN (SELECT data_json::jsonb->>'guestId' FROM pms_records
        WHERE property_id=? AND kind='stay' AND ${stayPredicate}))
    ) ORDER BY updated_at,id`;
export async function readPmsDashboard(db: SqlDatabase, propertyId: string, date: string) {
  const records = await readRecords(db, pmsDashboardRecordsSql, [propertyId, date, date, propertyId, date, date]);
  const exists = await db.prepare("SELECT EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='stay') present")
    .bind(propertyId).first<{ present: boolean }>();
  return { records, hasStays: exists?.present ?? false };
}

export function readPmsCalendar(db: SqlDatabase, propertyId: string, from: string, to: string) {
  return readRecords(db, `SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND (
    kind IN ('room_type','room')
    OR (kind='rate' AND data_json::jsonb->>'from'<=? AND data_json::jsonb->>'to'>=?)
    OR (kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house','checked_out')
      AND data_json::jsonb->>'arrival'<? AND data_json::jsonb->>'departure'>?)
    OR (kind='block' AND data_json::jsonb->>'state'='active' AND data_json::jsonb->>'from'<?
      AND (data_json::jsonb->>'kind'='maintenance' OR data_json::jsonb->>'to'>?))
    ) ORDER BY updated_at,id`, [propertyId, to, from, to, from, to, from]);
}

export function readPmsDetail(db: SqlDatabase, propertyId: string, id: string) {
  return readRecords(db, `WITH selected AS (
      SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id=?
    ), related AS (
      SELECT id FROM selected UNION
      SELECT value FROM selected CROSS JOIN LATERAL jsonb_each_text(data)
        WHERE key IN ('guestId','reservationId','folioId','roomId','typeId','rateId')
    ) SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=?
      AND (id IN (SELECT id FROM related) OR (kind='entry' AND data_json::jsonb->>'folioId' IN
        (SELECT id FROM selected WHERE kind='folio' UNION SELECT data->>'folioId' FROM selected)))
    ORDER BY updated_at,id`, [propertyId, id, propertyId]);
}
