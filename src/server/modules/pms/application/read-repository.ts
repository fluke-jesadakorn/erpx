import { plusDays, type Kind, type PmsRecord } from "../domain/pms-model";
import type { PmsReadSelection } from "@/lib/pms/pms-read-selection";
import type { PmsSetupFacts } from "@/lib/pms/pms-read";
export type PmsReadRows<Row> = {
    results: Row[];
};
export type PmsRoomSummary = {total:number;active:number;dirty:number;clean:number;inspected:number;dnd:number;occupied:number};
export type PmsReadRepository = {
    roomSummary(facts:{propertyId:string;businessDate:string;status:string;q:string;building:string;floor:string;typeId:string}):Promise<PmsRoomSummary|null>;
    operationsPage(facts: {
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
    }): Promise<PmsReadRows<{
        id: string;
    }>>;
    sectionPage(facts: {
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
    }): Promise<PmsReadRows<{
        id: string;
    }>>;
    setupFacts(facts: {
        propertyId: string;
        propertyId2: string;
        propertyId3: string;
    }): Promise<PmsSetupFacts | null>;
    hasStays(facts: {
        propertyId: string;
    }): Promise<{
        present: boolean;
    } | null>;
    allRecords(facts: {
        propertyId: string;
    }): Promise<PmsRecord[]>;
    configuration(facts: {
        propertyId: string;
        propertyId2: string;
    }): Promise<PmsRecord[]>;
    operationRelatedRecords(facts: {
        propertyId: string;
        seedsJson: string;
        propertyId2: string;
        propertyId3: string;
    }): Promise<PmsRecord[]>;
    sectionRelatedRecords(facts: {
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
    }): Promise<PmsRecord[]>;
    dashboard(facts: {
        propertyId: string;
        date: string;
        date2: string;
        propertyId2: string;
        date3: string;
        date4: string;
    }): Promise<PmsRecord[]>;
    calendar(facts: {
        propertyId: string;
        to: string;
        from: string;
        to2: string;
        from2: string;
        to3: string;
        from3: string;
    }): Promise<PmsRecord[]>;
    detail(facts: {
        propertyId: string;
        id: string;
        propertyId2: string;
    }): Promise<PmsRecord[]>;
};
export function readAllPmsRecords(db: PmsReadRepository, propertyId: string) {
    return db.allRecords({ propertyId: propertyId });
}
export function readPmsConfiguration(db: PmsReadRepository, propertyId: string) {
    return db.configuration({ propertyId: propertyId, propertyId2: propertyId });
}
export async function readPmsOperationsList(db: PmsReadRepository, propertyId: string, businessDate: string, selection: Extract<PmsReadSelection, {
    view: 'list';
}>) {
    if (selection.section !== 'reservations' && selection.section !== 'front-desk')
        return readPmsSectionList(db, propertyId, businessDate, selection);
    const from = selection.from ?? plusDays(businessDate, -30), to = selection.to ?? plusDays(from, 90), kind = selection.section === 'reservations' ? 'reservation' : 'stay';
    if (from > to || Date.parse(to) - Date.parse(from) > 93 * 86400000)
        throw new Error('PMS_LIST_RANGE_INVALID');
    const selected = (await db.operationsPage({ propertyId: propertyId, kind: kind, status: selection.status, status2: selection.status, status3: selection.status, businessDate: businessDate, status4: selection.status, businessDate2: businessDate, status5: selection.status, to: to, from: from, to2: to, from2: from, q: selection.q, q2: selection.q, offset: selection.page * 50 })).results;
    const ids = selected.slice(0, 50).map(r => r.id), seeds = [...ids, ...(selection.selectedId ? [selection.selectedId] : [])];
    const records = await db.operationRelatedRecords({ propertyId: propertyId, seedsJson: JSON.stringify(seeds), propertyId2: propertyId, propertyId3: propertyId });
    return { records, listMeta: { section: selection.section, kind, ids, from, to, page: selection.page, q: selection.q, status: selection.status, building: (selection.building??""), floor: (selection.floor??""), typeId: (selection.typeId??""), hasNext: selected.length > 50, selectedId: selection.selectedId } };
}
/** Primary records are paged; related facts never expand into the hotel's history. */
async function readPmsSectionList(db: PmsReadRepository, propertyId: string, businessDate: string, selection: Extract<PmsReadSelection, {
    view: 'list';
}>) {
    const section = selection.section;
    const kind: Kind = section === 'rooms' || section === 'housekeeping' ? 'room' : section === 'guest-accounts' ? 'guest_account' : section === 'cashier' ? 'shift' : section === 'inventory' ? 'item' : section === 'guests' ? 'guest' : 'night';
    const from = selection.from ?? plusDays(businessDate, -30), to = selection.to ?? plusDays(from, 90);
    if (from > to || Date.parse(to) - Date.parse(from) > 93 * 86400000)
        throw new Error('PMS_LIST_RANGE_INVALID');
    const selected = (await db.sectionPage({ propertyId: propertyId, kind: kind, status: selection.status, status2: selection.status, q: selection.q, q2: selection.q, building: (selection.building??""), floor: (selection.floor??""), typeId: (selection.typeId??""), from: from, to: to, offset: selection.page * 50 })).results;
    const ids = selected.slice(0, 50).map(row => row.id), seeds = [...ids, ...(selection.selectedId ? [selection.selectedId] : [])];
    const roomView = section === 'rooms' || section === 'housekeeping', nightView = section === 'night-audit', inventoryView = section === 'inventory';
    const records = await db.sectionRelatedRecords({ propertyId: propertyId, seedsJson: JSON.stringify(seeds), propertyId2: propertyId, propertyId3: propertyId, roomView: roomView, inventoryView: inventoryView, nightView: nightView, businessDate: businessDate, propertyId4: propertyId, roomView2: roomView, propertyId5: propertyId, inventoryView2: inventoryView, from: from, to: to, from2: from, to2: to, propertyId6: propertyId, inventoryView3: inventoryView, inventoryView4: inventoryView, propertyId7: propertyId, roomView3: roomView, nightView2: nightView, roomView4: roomView, roomView5: roomView, fact24: section === 'guest-accounts', nightView3: nightView, nightView4: nightView });
    const roomSummary = roomView ? await db.roomSummary({propertyId,businessDate,status:selection.status,q:selection.q,building:(selection.building??""),floor:(selection.floor??""),typeId:selection.typeId??""}) : undefined;
    return { records, listMeta: {roomSummary:roomSummary??undefined, section, kind, ids, from, to, page: selection.page, q: selection.q, status: selection.status, building: (selection.building??""), floor: (selection.floor??""), typeId: (selection.typeId??""), hasNext: selected.length > 50, selectedId: selection.selectedId } };
}
/** Setup readiness describes the property, independently of a bounded view. */
export async function readPmsSetupFacts(db: PmsReadRepository, propertyId: string): Promise<PmsSetupFacts> {
    return (await db.setupFacts({ propertyId: propertyId, propertyId2: propertyId, propertyId3: propertyId }))!;
}
// These readers are query models only. Commands must always load the complete,
// authoritative property state while holding its transaction lock.
export async function readPmsDashboard(db: PmsReadRepository, propertyId: string, date: string) {
    const records = await db.dashboard({ propertyId: propertyId, date: date, date2: date, propertyId2: propertyId, date3: date, date4: date });
    const exists = await db.hasStays({ propertyId: propertyId });
    return { records, hasStays: exists?.present ?? false };
}
export function readPmsCalendar(db: PmsReadRepository, propertyId: string, from: string, to: string) {
    return db.calendar({ propertyId: propertyId, to: to, from: from, to2: to, from2: from, to3: to, from3: from });
}
export function readPmsDetail(db: PmsReadRepository, propertyId: string, id: string) {
    return db.detail({ propertyId: propertyId, id: id, propertyId2: propertyId });
}
