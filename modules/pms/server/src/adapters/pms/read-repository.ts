import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { PmsReadSelection } from "@erpx/pms-ui/lib/pms-read-selection";
import type { PmsSetupFacts } from "@erpx/pms-ui/lib/pms-read";
import { decodePmsRecord, type PmsStoredRecord } from "@erpx/pms-server/domain/stored-record";
import { PMS_RECORDS_PROJECTION_PREFIX_SQL } from "../pms-read-tail-r24-sql";
import { pmsBookingEmailPendingSql } from "../pms-read-tail-r43-sql";
import { nativePmsReadAll, PMS_RD_INVENTORY_COMMAND, PMS_RD_SECTION_RELATED_RECORDS, PMS_RD_DASHBOARD_RECORDS, PMS_RD_RECORD_DETAIL, PMS_RD_PMS_CHOICES, PMS_RD_PMS_CHOICES_BLOCK, PMS_RD_OPERATION_RELATED_RECORDS, PMS_RD_OPERATIONS_PAGE, PMS_RD_SECTION_PAGE } from "../native-pms-read";
import { propertyReadinessFacts, type LaneActor } from "../native-lane-20/client";
import { roomSummaryFacts } from "../native-lane-20/room-summary-client";
import * as application from "@erpx/pms-server/application/read-repository";
function repository(db: SqlDatabase, actor: LaneActor = {}): application.PmsReadRepository {
    return {
        // Room aggregates belong to the native axum owner. No SQL is issued
        // here; the owner scopes by Company and projects the bounded counts.
        roomSummary: async (facts: {
            propertyId: string;
            businessDate: string;
            status: string;
            q: string;
            building: string;
            floor: string;
            typeId: string;
        }): Promise<application.PmsRoomSummary | null> => roomSummaryFacts(db, actor, {
            propertyId: facts.propertyId,
            status: facts.status,
            q: facts.q,
            building: facts.building,
            floor: facts.floor,
            typeId: facts.typeId,
        }),
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
        }) => nativePmsReadAll<{ id: string }>(PMS_RD_OPERATIONS_PAGE, [facts.propertyId, facts.kind, facts.status, facts.status2, facts.status3, facts.businessDate, facts.status4, facts.businessDate2, facts.status5, facts.to, facts.from, facts.to2, facts.from2, facts.q, facts.q2, facts.offset], db),
        sectionPage: (facts: {
            propertyId: string;
            kind: "shift" | "room" | "guest" | "guest_account" | "item" | "night";
            status: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            status2: "" | "arrivals" | "departures" | "reserved" | "in_house" | "cancelled" | "no_show" | "completed" | "checked_out" | "payment_pending" | "confirmed" | "open" | "closed" | "reviewed" | "dirty" | "clean" | "inspected" | "running" | "blocked";
            q: string;
            q2: string;
        building: string;
        floor: string;
        typeId: string;
            from: string;
            to: string;
            offset: number;
        }) => nativePmsReadAll<{ id: string }>(PMS_RD_SECTION_PAGE, [facts.propertyId, facts.kind, facts.status, facts.status2, facts.q, facts.q2, facts.building, facts.building, facts.floor, facts.floor, facts.typeId, facts.typeId, facts.from, facts.to, facts.offset], db),
        // Setup/stay readiness is owned by the native axum reader. No SQL is
        // issued here; the owner scopes by Company and projects the booleans.
        setupFacts: async (facts: {
            propertyId: string;
            propertyId2: string;
            propertyId3: string;
        }): Promise<PmsSetupFacts | null> => {
            const readiness = await propertyReadinessFacts(db, actor, facts.propertyId);
            if (!readiness) return null;
            return {
                hasRoomTypes: readiness.hasRoomTypes,
                hasActiveRooms: readiness.hasActiveRooms,
                hasActiveRates: readiness.hasActiveRates,
            };
        },
        hasStays: async (facts: {
            propertyId: string;
        }): Promise<{ present: boolean } | null> => {
            const readiness = await propertyReadinessFacts(db, actor, facts.propertyId);
            return readiness ? { present: readiness.hasStays } : null;
        },
        allRecords: (facts: {
            propertyId: string;
        }) => db.prepare(`${PMS_RECORDS_PROJECTION_PREFIX_SQL} WHERE property_id=? ORDER BY updated_at,id`).bind(facts.propertyId).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        configuration: (facts: {
            propertyId: string;
            propertyId2: string;
        }) => db.prepare(`${PMS_RECORDS_PROJECTION_PREFIX_SQL} WHERE property_id=? AND (
   kind IN ('room_type','room','rate') OR (kind='reservation' AND id IN (
     ${pmsBookingEmailPendingSql('reservationId')}
   ))) ORDER BY updated_at,id`).bind(facts.propertyId, facts.propertyId2).all<PmsStoredRecord>().then(rows => rows.results.map(decodePmsRecord)),
        operationRelatedRecords: (facts: {
            propertyId: string;
            seedsJson: string;
            propertyId2: string;
            propertyId3: string;
        }) => nativePmsReadAll<PmsStoredRecord>(PMS_RD_OPERATION_RELATED_RECORDS, [facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3, facts.propertyId3], db).then(rows => rows.results.map(decodePmsRecord)),
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
        }) => nativePmsReadAll<PmsStoredRecord>(PMS_RD_SECTION_RELATED_RECORDS, [facts.propertyId, facts.seedsJson, facts.propertyId2, facts.propertyId3, facts.propertyId3, facts.roomView, facts.inventoryView, facts.nightView, facts.businessDate, facts.propertyId4, facts.roomView2, facts.propertyId5, facts.inventoryView2, facts.from, facts.to, facts.from2, facts.to2, facts.propertyId6, facts.inventoryView3, facts.inventoryView4, facts.propertyId7, facts.roomView3, facts.nightView2, facts.roomView4, facts.roomView5, facts.fact24, facts.nightView3, facts.nightView4, facts.propertyId6], db)
            .then(rows => rows.results.map(decodePmsRecord)),
        dashboard: (facts: {
            propertyId: string;
            date: string;
            date2: string;
            propertyId2: string;
            date3: string;
            date4: string;
        }) => nativePmsReadAll<PmsStoredRecord>(PMS_RD_DASHBOARD_RECORDS, [facts.propertyId, facts.date, facts.date2, facts.propertyId2], db).then(rows => rows.results.map(decodePmsRecord)),
        calendar: (facts: {
            propertyId: string;
            to: string;
            from: string;
            to2: string;
            from2: string;
            to3: string;
            from3: string;
        }) => db.prepare(`${PMS_RECORDS_PROJECTION_PREFIX_SQL} WHERE property_id=? AND (
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
        }) => nativePmsReadAll<PmsStoredRecord>(PMS_RD_RECORD_DETAIL, [facts.propertyId, facts.id, facts.propertyId2], db).then(rows => rows.results.map(decodePmsRecord))
    };
}
export function readAllPmsRecords(db: SqlDatabase, propertyId: string) { return application.readAllPmsRecords(repository(db), propertyId); }
export function readPmsConfiguration(db: SqlDatabase, propertyId: string) { return application.readPmsConfiguration(repository(db), propertyId); }
export function readPmsOperationsList(db: SqlDatabase, propertyId: string, businessDate: string, selection: Extract<PmsReadSelection, {
    view: 'list';
}>) { return application.readPmsOperationsList(repository(db), propertyId, businessDate, selection); }
/** Bounded selector facts only; callers retain Property scope and permission checks. */
export async function readPmsChoices(db:SqlDatabase,propertyId:string,selection:Extract<PmsReadSelection,{view:'choices'}>){
 const rows=await nativePmsReadAll<PmsStoredRecord>(
  selection.kind==='block'?PMS_RD_PMS_CHOICES_BLOCK:PMS_RD_PMS_CHOICES,
  [propertyId,selection.kind,selection.q,selection.q,propertyId,selection.kind,selection.selectedId??''], db);
 return rows.results.map(decodePmsRecord);
}
export function readPmsSetupFacts(db: SqlDatabase, actor: LaneActor, propertyId: string) { return application.readPmsSetupFacts(repository(db, actor), propertyId); }
export function readPmsDashboard(db: SqlDatabase, actor: LaneActor, propertyId: string, date: string) { return application.readPmsDashboard(repository(db, actor), propertyId, date); }
export function readPmsCalendar(db: SqlDatabase, propertyId: string, from: string, to: string) { return application.readPmsCalendar(repository(db), propertyId, from, to); }
export function readPmsDetail(db: SqlDatabase, propertyId: string, id: string) { return application.readPmsDetail(repository(db), propertyId, id); }

/** Authoritative configuration command facts; the caller owns Property scope/version/lock.
 * The owner plans this exact statement in Rust; no TypeScript SQL remains. */
export function readPmsInventoryCommand(db: SqlDatabase, propertyId: string) {
    return nativePmsReadAll<PmsStoredRecord>(PMS_RD_INVENTORY_COMMAND, [propertyId, propertyId], db)
        .then(rows=>rows.results.map(decodePmsRecord));
}
