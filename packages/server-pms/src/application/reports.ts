import { z } from "zod";
import { dateField, type Config } from "@erpx/server-pms/domain/pms";
import { pmsReportRange } from "@erpx/server-pms/domain/reports";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
export type PmsReportRows<Row> = {
    results: Row[];
};
export type PmsChannelReport = {source:string;reservations:number;room_nights:number;quoted_total:number;cancelled:number;no_shows:number;recent_pickup:number;unknown_creation:number};
export type PmsPacePoint={as_of:string;coverage_since:string|null;covered:boolean;reservations:number;room_nights:number;quoted_total:number};
export type PmsReportsRepository = {
    pace(facts:{propertyId:string;from:string;to:string;asOf:string|null;compareAsOf:string|null}):Promise<PmsReportRows<PmsPacePoint>>;
    channels(facts:{propertyId:string;from:string;to:string;asOf:string;timezone:string}):Promise<PmsReportRows<PmsChannelReport>>;
    property(facts: {
        propertyId: string;
        companyId: string;
    }): Promise<{
        branch_id: string;
        config_json: string;
    } | null>;
    days(facts: {
        from: string;
        to: string;
        propertyId: string;
        from2: string;
        to2: string;
        propertyId2: string;
        propertyId3: string;
        propertyId4: string;
        propertyId5: string;
        propertyId6: string;
        propertyId7: string;
        timezone: string;
        propertyId8: string;
        propertyId9: string;
    }): Promise<PmsReportRows<{
        date: string;
        capacity: number;
        sold: number;
        room_revenue: number;
        net_revenue: number;
        tax: number;
        refunds: number;
        cancellations: number;
        no_shows: number;
    }>>;
    balances(facts: {
        propertyId: string;
        to: string;
    }): Promise<PmsReportRows<{
        account_id: string;
        balance: number;
    }>>;
    entries(facts: {
        propertyId: string;
        date: string;
        offset: number;
    }): Promise<PmsReportRows<{
        id: string;
        data_json: string;
    }>>;
};
export function createPmsReportsApplication(ports: {
    clock:{isoNow():string};
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
}) {
    async function scope(db: PmsReportsRepository, actor: WorkContext, propertyId: string) {
        ports.require(actor, 'pms.read');
        ports.require(actor, 'pms.reports');
        const row = await db.property({ propertyId: propertyId, companyId: actor.companyId });
        if (!row || actor.departmentId || actor.beta || actor.branchId && actor.branchId !== row.branch_id)
            throw ports.error(404, 'PMS_NOT_FOUND', 'Hotel not found in this workspace');
        return JSON.parse(row.config_json) as Config;
    }
    async function pmsRangeReport(db: PmsReportsRepository, actor: WorkContext, raw: unknown) {
        const { propertyId, from, to,asOf:requestedAsOf,compareAsOf:requestedCompare } = pmsReportRange.parse(raw), config = await scope(db, actor, propertyId);
        const now=ports.clock.isoNow(),asOf=requestedAsOf??now,compareAsOf=requestedCompare??new Date(Date.parse(asOf)-7*86400000).toISOString();
        if(Date.parse(asOf)>Date.parse(now)||Date.parse(compareAsOf)>=Date.parse(asOf))throw ports.error(400,'PMS_PACE_RANGE','Choose a past observation and an earlier comparison time');
        const rows = (await db.days({ from: from, to: to, propertyId: propertyId, from2: from, to2: to, propertyId2: propertyId, propertyId3: propertyId, propertyId4: propertyId, propertyId5: propertyId, propertyId6: propertyId, propertyId7: propertyId, timezone: config.timezone, propertyId8: propertyId, propertyId9: propertyId })).results;

        const channels=(await db.channels({propertyId,from,to,asOf:now,timezone:config.timezone})).results;
        const points=(await db.pace({propertyId,from,to,asOf:requestedAsOf??null,compareAsOf:requestedCompare??null})).results;
        const current=points[0]??null,previous=points[1]??null;
        const pace={current,previous,netPickup:current?.covered&&previous?.covered?{reservations:Number(current.reservations)-Number(previous.reservations),roomNights:Number(current.room_nights)-Number(previous.room_nights),quotedTotal:Number(current.quoted_total)-Number(previous.quoted_total)}:null};
        const balances = (await db.balances({ propertyId: propertyId, to: to })).results;
        const balance = (account: string | undefined, sign = 1) => balances.filter(r => r.account_id === account).reduce((n, r) => n + Number(r.balance) * sign, 0);
        const totals = rows.reduce((n, r) => ({ capacity: n.capacity + Number(r.capacity), sold: n.sold + Number(r.sold), roomRevenue: n.roomRevenue + Number(r.room_revenue), netRevenue: n.netRevenue + Number(r.net_revenue), tax: n.tax + Number(r.tax), refunds: n.refunds + Number(r.refunds), cancellations: n.cancellations + Number(r.cancellations), noShows: n.noShows + Number(r.no_shows) }), { capacity: 0, sold: 0, roomRevenue: 0, netRevenue: 0, tax: 0, refunds: 0, cancellations: 0, noShows: 0 });
        return { from, to, asOf:now, channels, pace, currency: config.currency, days: rows, totals, occupancy: totals.capacity ? totals.sold / totals.capacity : 0, adr: totals.sold ? Math.round(totals.roomRevenue / totals.sold) : 0, revpar: totals.capacity ? Math.round(totals.roomRevenue / totals.capacity) : 0, balances: { receivable: balance(config.accounts.receivable), advance: balance(config.accounts.advance, -1), deposit: balance(config.accounts.deposit, -1), refundPayable: balance(config.onlineAccounting?.refundPayable, -1), unallocated: balance(config.onlineAccounting?.unallocated, -1) } };
    }
    async function pmsReportEntries(db: PmsReportsRepository, actor: WorkContext, propertyId: string, date: string, page: number) {
        dateField.parse(date);
        z.number().int().min(1).max(10000).parse(page);
        await scope(db, actor, propertyId);
        const rows = (await db.entries({ propertyId: propertyId, date: date, offset: (page - 1) * 50 })).results;
        return { rows: rows.slice(0, 50).map(r => ({ id: r.id, ...JSON.parse(r.data_json) })), hasNext: rows.length > 50, page };
    }
    return { pmsRangeReport, pmsReportEntries };
}
