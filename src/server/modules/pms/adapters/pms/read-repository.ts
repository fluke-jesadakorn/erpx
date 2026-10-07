import "server-only";
import type { SqlDatabase } from "../../../runtime/public/runtime/types";
import type { PmsReadSelection } from "@/lib/pms-read-selection";
import type { PmsSetupFacts } from "@/lib/pms-read";
import { decodePmsRecord, type PmsStoredRecord } from "../../domain/stored-record";
import * as application from "../../application/read-repository";
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
function repository(db: SqlDatabase): application.PmsReadRepository {
    return {
        operationsPage: (facts: {
            propertyId: string;
            kind: "reservation" | "stay";
            status: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            status2: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            status3: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            businessDate: string;
            status4: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            businessDate2: string;
            status5: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            to: string;
            from: string;
            to2: string;
            from2: string;
            q: string;
            q2: string;
            offset: number;
        }) => db.prepare(`SELECT r.id FROM pms_records r
   LEFT JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' AND g.property_id=r.property_id AND g.kind='guest'
   WHERE r.property_id=? AND r.kind=?
   AND (r.data_json::jsonb->>'state'=? OR ?='' OR (?='arrivals' AND r.data_json::jsonb->>'state'='reserved' AND r.data_json::jsonb->>'arrival'=?) OR (?='departures' AND r.data_json::jsonb->>'state'='in_house' AND r.data_json::jsonb->>'departure'<=?))
   AND (?<>'' OR r.kind='reservation' OR r.data_json::jsonb->>'state' IN ('reserved','in_house'))
   AND (CASE WHEN r.kind='stay' THEN (r.data_json::jsonb->>'arrival'<=? AND r.data_json::jsonb->>'departure'>=?) OR r.data_json::jsonb->>'state'='in_house'
     ELSE EXISTS(SELECT 1 FROM pms_records st WHERE st.property_id=r.property_id AND st.kind='stay' AND st.data_json::jsonb->>'reservationId'=r.id AND ((st.data_json::jsonb->>'arrival'<=? AND st.data_json::jsonb->>'departure'>=?) OR st.data_json::jsonb->>'state'='in_house')) END)
   AND (?='' OR position(lower(?) in lower(concat_ws(' ',g.data_json::jsonb->>'name',r.id,r.data_json::jsonb->>'number',(SELECT data_json::jsonb->>'number' FROM pms_records parent WHERE parent.property_id=r.property_id AND parent.id=r.data_json::jsonb->>'reservationId'))))>0)
   ORDER BY COALESCE(r.data_json::jsonb->>'arrival',r.updated_at::text),r.id LIMIT 51 OFFSET ?`).bind(facts.propertyId, facts.kind, facts.status, facts.status2, facts.status3, facts.businessDate, facts.status4, facts.businessDate2, facts.status5, facts.to, facts.from, facts.to2, facts.from2, facts.q, facts.q2, facts.offset).all<{
            id: string;
        }>(),
        sectionPage: (facts: {
            propertyId: string;
            kind: "shift" | "room" | "guest" | "folio" | "item" | "night";
            status: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            status2: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            q: string;
            q2: string;
            from: string;
            to: string;
            offset: number;
        }) => db.prepare(`SELECT id FROM pms_records WHERE property_id=? AND kind=?
   AND (?='' OR COALESCE(data_json::jsonb->>'cleanliness',data_json::jsonb->>'state')=?)
   AND (?='' OR position(lower(?) in lower(concat_ws(' ',id,data_json::jsonb->>'name',data_json::jsonb->>'number',data_json::jsonb->>'phone',data_json::jsonb->>'email',data_json::jsonb->>'building',data_json::jsonb->>'floor')))>0)
   AND (kind NOT IN ('shift','night') OR data_json::jsonb->>'date' BETWEEN ? AND ? OR data_json::jsonb->>'state' IN ('open','running','blocked'))
   ORDER BY CASE WHEN kind='room' THEN data_json::jsonb->>'number' WHEN kind='guest' THEN data_json::jsonb->>'name' ELSE '' END,updated_at DESC,id LIMIT 51 OFFSET ?`).bind(facts.propertyId, facts.kind, facts.status, facts.status2, facts.q, facts.q2, facts.from, facts.to, facts.offset).all<{
            id: string;
        }>(),
        setupFacts: (facts: {
            propertyId: string;
            propertyId2: string;
            propertyId3: string;
        }) => db.prepare(`SELECT
    EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='room_type') AS "hasRoomTypes",
    EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='room' AND data_json::jsonb->>'active'='true') AS "hasActiveRooms",
    EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='rate' AND data_json::jsonb->>'active'='true') AS "hasActiveRates"`).bind(facts.propertyId, facts.propertyId2, facts.propertyId3).first<PmsSetupFacts>(),
        hasStays: (facts: {
            propertyId: string;
        }) => db.prepare("SELECT EXISTS(SELECT 1 FROM pms_records WHERE property_id=? AND kind='stay') present").bind(facts.propertyId).first<{
            present: boolean;
        }>(),
        allRecords: (facts: {
            propertyId: string;
        }) => db.prepare("SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? ORDER BY updated_at,id").bind(facts.propertyId).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        configuration: (facts: {
            propertyId: string;
            propertyId2: string;
        }) => db.prepare(`SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND (
   kind IN ('room_type','room','rate') OR (kind='reservation' AND id IN (
     SELECT reservation_id FROM pms_booking_emails WHERE property_id=? AND state!='sent' ORDER BY created_at DESC LIMIT 50
   ))) ORDER BY updated_at,id`).bind(facts.propertyId, facts.propertyId2).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        operationRelatedRecords: (facts: {
            propertyId: string;
            seedsJson: string;
            propertyId2: string;
            propertyId3: string;
        }) => db.prepare(`WITH chosen AS (SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id IN (SELECT jsonb_array_elements_text(?::jsonb))),
   reservations AS (SELECT id FROM chosen WHERE kind='reservation' UNION SELECT data->>'reservationId' FROM chosen),
   stays AS (SELECT id,data_json::jsonb data FROM pms_records WHERE property_id=? AND kind='stay' AND (id IN (SELECT id FROM chosen) OR data_json::jsonb->>'reservationId' IN (SELECT id FROM reservations))),
   folios AS (SELECT data->>'folioId' id FROM stays)
   SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND (
    kind IN ('room_type','room','rate') OR id IN (SELECT id FROM chosen) OR id IN (SELECT id FROM reservations) OR id IN (SELECT id FROM stays) OR id IN (SELECT id FROM folios)
    OR (kind='guest' AND id IN (SELECT data->>'guestId' FROM stays UNION SELECT data->>'guestId' FROM chosen))
    OR (kind='entry' AND data_json::jsonb->>'folioId' IN (SELECT id FROM folios))) ORDER BY updated_at,id`).bind(facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        sectionRelatedRecords: (facts: {
            propertyId: string;
            seedsJson: string;
            propertyId2: string;
            propertyId3: string;
            roomView: boolean;
            inventoryView: boolean;
            nightView: boolean;
            businessDate: string;
            propertyId4: string;
            roomView2: boolean;
            propertyId5: string;
            inventoryView2: boolean;
            from: string;
            to: string;
            from2: string;
            to2: string;
            propertyId6: string;
            inventoryView3: boolean;
            inventoryView4: boolean;
            propertyId7: string;
            roomView3: boolean;
            nightView2: boolean;
            roomView4: boolean;
            roomView5: boolean;
            fact24: boolean;
            nightView3: boolean;
            nightView4: boolean;
        }) => db.prepare(`WITH chosen AS (
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
 ) ORDER BY updated_at,id`).bind(facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3, facts.roomView, facts.inventoryView, facts.nightView, facts.businessDate, facts.propertyId4, facts.roomView2, facts.propertyId5, facts.inventoryView2, facts.from, facts.to, facts.from2, facts.to2, facts.propertyId6, facts.inventoryView3, facts.inventoryView4, facts.propertyId7, facts.roomView3, facts.nightView2, facts.roomView4, facts.roomView5, facts.fact24, facts.nightView3, facts.nightView4).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        dashboard: (facts: {
            propertyId: string;
            date: string;
            date2: string;
            propertyId2: string;
            date3: string;
            date4: string;
        }) => db.prepare(pmsDashboardRecordsSql).bind(facts.propertyId, facts.date, facts.date2, facts.propertyId2, facts.date3, facts.date4).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        calendar: (facts: {
            propertyId: string;
            to: string;
            from: string;
            to2: string;
            from2: string;
            to3: string;
            from3: string;
        }) => db.prepare(`SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND (
    kind IN ('room_type','room')
    OR (kind='rate' AND data_json::jsonb->>'from'<=? AND data_json::jsonb->>'to'>=?)
    OR (kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house','checked_out')
      AND data_json::jsonb->>'arrival'<? AND data_json::jsonb->>'departure'>?)
    OR (kind='block' AND data_json::jsonb->>'state'='active' AND data_json::jsonb->>'from'<?
      AND (data_json::jsonb->>'kind'='maintenance' OR data_json::jsonb->>'to'>?))
    ) ORDER BY updated_at,id`).bind(facts.propertyId, facts.to, facts.from, facts.to2, facts.from2, facts.to3, facts.from3).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        detail: (facts: {
            propertyId: string;
            id: string;
            propertyId2: string;
        }) => db.prepare(`WITH selected AS (
      SELECT id,kind,data_json::jsonb data FROM pms_records WHERE property_id=? AND id=?
    ), related AS (
      SELECT id FROM selected UNION
      SELECT value FROM selected CROSS JOIN LATERAL jsonb_each_text(data)
        WHERE key IN ('guestId','reservationId','folioId','roomId','typeId','rateId')
    ) SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=?
      AND (id IN (SELECT id FROM related) OR (kind='entry' AND data_json::jsonb->>'folioId' IN
        (SELECT id FROM selected WHERE kind='folio' UNION SELECT data->>'folioId' FROM selected)))
    ORDER BY updated_at,id`).bind(facts.propertyId, facts.id, facts.propertyId2).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord))
    };
}
export function readAllPmsRecords(db: SqlDatabase, propertyId: string) { return application.readAllPmsRecords(repository(db), propertyId); }
export function readPmsConfiguration(db: SqlDatabase, propertyId: string) { return application.readPmsConfiguration(repository(db), propertyId); }
export function readPmsOperationsList(db: SqlDatabase, propertyId: string, businessDate: string, selection: Extract<PmsReadSelection, {
    view: 'list';
}>) { return application.readPmsOperationsList(repository(db), propertyId, businessDate, selection); }
export function readPmsSetupFacts(db: SqlDatabase, propertyId: string) { return application.readPmsSetupFacts(repository(db), propertyId); }
export function readPmsDashboard(db: SqlDatabase, propertyId: string, date: string) { return application.readPmsDashboard(repository(db), propertyId, date); }
export function readPmsCalendar(db: SqlDatabase, propertyId: string, from: string, to: string) { return application.readPmsCalendar(repository(db), propertyId, from, to); }
export function readPmsDetail(db: SqlDatabase, propertyId: string, id: string) { return application.readPmsDetail(repository(db), propertyId, id); }
