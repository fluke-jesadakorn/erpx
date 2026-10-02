import "server-only";
import type { Kind, PmsRecord } from "@/lib/pms-model";
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
    OR (kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house')
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
