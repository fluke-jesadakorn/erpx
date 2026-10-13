import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkItem } from "@/lib/work_projection/work-item";
import type { Clock } from "../../shared/application/ports";
import type { WorkContext } from "../../identity_access/public/authorization";
import { taskView, type TaskRow, type PmsHomeView, type PmsHotelOverview } from "../domain/home";
export type PmsHomeRows<Row> = {
    results: Row[];
};
export type PmsHomePost = Awaited<ReturnType<typeof import("../../collaboration/public/social-home").postView>>;
export type PmsTaskPageOptions = {
    propertyId?: string;
    mine?: boolean;
    label?: string;
    cursor?: string;
    limit?: number;
};
export type PmsHomeRepository = {
    hotelMetrics(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        propertyId: string | null;
        propertyId2: string | null;
    }): Promise<PmsHomeRows<{
        id: string;
        name: string;
        branch_id: string;
        business_date: string;
        state: string;
        currency: string;
        room_count: number;
        inspected: number;
        capacity: number;
        sold: number;
        arrivals: number;
        departures: number;
    }>>;
    hotelMoney(facts: {
        companyId: string;
        branchId: string | null;
        branchId2: string | null;
        companyId2: string;
        branchId3: string | null;
        branchId4: string | null;
        propertyId: string | null;
        propertyId2: string | null;
    }): Promise<PmsHomeRows<{
        id: string;
        room_revenue: number;
        receivable: number;
        advance: number;
        deposit: number;
    }>>;
    hotelSources(facts: {
        id: string;
        id2: string;
        id3: string;
        metric: "occupancy" | "adr" | "revpar" | "room_revenue";
        businessDate: string;
        metric2: "occupancy" | "adr" | "revpar" | "room_revenue";
        businessDate2: string;
        businessDate3: string;
        metric3: "occupancy" | "adr" | "revpar" | "room_revenue";
        businessDate4: string;
        offset: number;
    }): Promise<PmsHomeRows<{
        id: string;
        kind: string;
        version: number;
        data_json: string;
    }>>;
    taskPage(actor: WorkContext, options: PmsTaskPageOptions): Promise<{
        items: TaskRow[];
        nextCursor: string | null;
        previousCursor: string | null;
        hasMore: boolean;
    }>;
    tasks(actor: WorkContext): Promise<TaskRow[]>;
    readablePosts(actor: WorkContext, loadedIds: string[], now: number): Promise<PmsHomeRows<{
        id: string;
    }>>;
    counts(actor: WorkContext, propertyId: string | undefined, mine: boolean): Promise<PmsHomeRows<{
        label: string;
        count: number;
        urgent: number;
    }>>;
    post(actor: WorkContext, id: string): Promise<PmsHomePost>;
    posts(actor: WorkContext, options: {
        limit: number;
        before?: number;
        beforeId?: string;
        propertyId?: string;
        purpose?: "handover";
    }): Promise<PmsHomePost[]>;
    recipients(actor: WorkContext): Promise<{
        id: string;
        name: string;
    }[]>;
};
export function createPmsHomeApplication(ports: {
    resource(holdingId: string): Promise<{
        write: PmsHomeRepository;
    }>;
    clock: Clock;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
}) {
    async function pmsTaskPage(db: PmsHomeRepository, actor: WorkContext, options: {
        propertyId?: string;
        mine?: boolean;
        label?: string;
        cursor?: string;
        limit?: number;
    } = {}) {
        const page = await db.taskPage(actor, options);
        return { ...page, items: page.items.map(row => taskView(row, actor)) };
    }
    /** Work reuses source IDs and policy, without loading historical guestAccounts/entries. */
    async function pmsOperationalWork(db: PmsHomeRepository, actor: WorkContext): Promise<WorkItem[]> {
        const rows = await db.tasks(actor);
        return rows.map(row => taskView(row, actor));
    }
    async function pmsHotelOverview(db: PmsHomeRepository, actor: WorkContext, propertyId?: string): Promise<PmsHotelOverview[]> {
        const financial = hasPermission(actor, "pms.reports");
        const rows = (await db.hotelMetrics({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, propertyId: propertyId ?? null, propertyId2: propertyId ?? null })).results;
        const money = financial ? (await db.hotelMoney({ companyId: actor.companyId, branchId: actor.branchId, branchId2: actor.branchId, companyId2: actor.companyId, branchId3: actor.branchId, branchId4: actor.branchId, propertyId: propertyId ?? null, propertyId2: propertyId ?? null })).results : [];
        return rows.map(row => {
            const financialRow = money.find(item => item.id === row.id);
            const capacity = Number(row.capacity), sold = Number(row.sold), revenue = financialRow ? Number(financialRow.room_revenue) : null;
            return { id: row.id, name: row.name, branchId: row.branch_id, businessDate: row.business_date, state: row.state, currency: row.currency,
                capacity, sold, roomCount: Number(row.room_count), inspected: Number(row.inspected), arrivals: Number(row.arrivals), departures: Number(row.departures),
                occupancy: capacity ? sold / capacity : null, roomRevenue: revenue, adr: revenue !== null && sold ? revenue / sold : null, revpar: revenue !== null && capacity ? revenue / capacity : null,
                receivable: financialRow ? Number(financialRow.receivable) : null, advance: financialRow ? Number(financialRow.advance) : null, deposit: financialRow ? Number(financialRow.deposit) : null };
        });
    }
    async function pmsHome(actor: WorkContext, options: {
        propertyId?: string;
        view?: PmsHomeView;
        label?: string;
        taskCursor?: string;
        before?: number;
        beforeId?: string;
        postId?: string;
        loadedIds?: string[];
    } = {}) {
        ports.require(actor, "pms.read");
        if (actor.departmentId || actor.beta)
            throw ports.error(403, "PMS_BRANCH_REQUIRED", "Choose a company or branch.");
        const { write } = await ports.resource(actor.holdingId);
        const hotels = await pmsHotelOverview(write, actor, options.propertyId);
        if (options.propertyId && !hotels.length)
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found in this workspace.");
        const loadedIds = options.loadedIds ?? [], now = ports.clock.now();
        const readableIds = loadedIds.length ? (await write.readablePosts(actor, loadedIds, now)).results.map(post => post.id) : [];
        const [tasks, counts, posts, people] = await Promise.all([
            pmsTaskPage(write, actor, { propertyId: options.propertyId, mine: options.view === "mine", label: options.label, cursor: options.taskCursor }),
            write.counts(actor, options.propertyId, options.view === "mine"),
            options.postId ? write.post(actor, options.postId).then(post => [post]) : write.posts(actor, { limit: 8, before: options.before, beforeId: options.beforeId,
                propertyId: options.propertyId, purpose: options.view === "handover" ? "handover" : undefined }),
            write.recipients(actor),
        ]);
        return { updatedAt: ports.clock.now(), hotels, tasks, counts: counts.results.map(row => ({ label: row.label, count: Number(row.count), urgent: Number(row.urgent) })), posts, readablePostIds: readableIds,
            canFinance: hasPermission(actor, "finance.read") && hasPermission(actor, "pms.reports"),
            feedHasMore: posts.length === 8, people, userId: actor.userId, view: options.view ?? "team" };
    }
    /** Rooms, stays and room-charge lineage exactly mirror the current hotel overview. */
    async function pmsHotelSources(actor: WorkContext, id: string, metric: 'occupancy' | 'adr' | 'revpar' | 'room_revenue', businessDate: string, page = 1) {
        ports.require(actor, 'pms.read');
        if (actor.departmentId || actor.beta)
            throw ports.error(403, 'PMS_BRANCH_REQUIRED', 'Choose a Company or Branch.');
        if (metric !== 'occupancy')
            ports.require(actor, 'pms.reports');
        const { write } = await ports.resource(actor.holdingId), hotel = (await pmsHotelOverview(write, actor, id))[0];
        if (!hotel)
            throw ports.error(404, 'PMS_NOT_FOUND', 'Hotel not found in this workspace.');
        if (hotel.businessDate !== businessDate)
            throw ports.error(409, 'PMS_BUSINESS_DATE_CHANGED', 'The hotel business date changed. Reload the overview.');
        const rows = (await write.hotelSources({ id: id, id2: id, id3: id, metric: metric, businessDate: businessDate, metric2: metric, businessDate2: businessDate, businessDate3: businessDate, metric3: metric, businessDate4: businessDate, offset: (page - 1) * 30 })).results;
        return { hotel, metric, rows: rows.slice(0, 30).map(row => ({ id: row.id, kind: row.kind, version: row.version, data: Object.fromEntries(Object.entries(JSON.parse(row.data_json) as Record<string, unknown>).filter(([key]) => ['name', 'number', 'description', 'kind', 'state', 'cleanliness', 'active', 'roomId', 'guestAccountId', 'arrival', 'departure', 'date', 'reference', 'quantity', 'net', 'service', 'tax', 'total'].includes(key))) })), page, hasNext: rows.length > 30 };
    }
    return { pmsTaskPage, pmsOperationalWork, pmsHotelOverview, pmsHome, pmsHotelSources };
}
