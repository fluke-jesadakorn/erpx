import {calendarTodayAt} from "@erpx/server-work_projection/public/rules/calendar-date-domain";
import { createCalendarQuerySchema, calendarRange, calendarLayers, type CalendarData, type CalendarItem, type CalendarLayer } from "@erpx/server-work_projection/public/rules/calendar-domain";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { descendants } from "@erpx/server-work_projection/domain/calendar/access";
import type { CalendarAccess } from "@erpx/server-work_projection/domain/calendar/access";
const idList = (ids: string[]) => JSON.stringify(ids);
import type { Clock } from "@erpx/server-shared/application/ports";
import type { CalendarQuery } from "@erpx/server-work_projection/public/rules/calendar-domain";
export type CalendarLayerRequest = {
    actor: WorkContext;
    access: CalendarAccess;
    query: CalendarQuery;
    from: string;
    to: string;
    fromMs: number;
    toMs: number;
    take: number;
};
export type CalendarIndexUnit = {
    access(actor: WorkContext): Promise<CalendarAccess>;
    layer(layer: CalendarLayer, input: CalendarLayerRequest): Promise<CalendarItem[]>;
    warnings(companyId: string, ids: string[], start: number, end: number, zone: string, except: string, userIds: string[]): Promise<string[]>;
    read1(facts: {
        value1: string;
        value2: string;
        value3: string;
    }): Promise<unknown>;
    read2(facts: {
        value1: string;
        value2: string;
        value3: string;
    }): Promise<{
        from_ms: number;
        to_ms: number;
    } | null>;
    read3(facts: {
        value1: string;
        value2: string;
        value3: number;
        value4: string;
        value5: number;
    }): Promise<unknown>;
    read4(facts: {
        value1: string;
        value2: string;
    }): Promise<{
        record_id: string;
        action: string;
        actor: string;
        at: number;
    }[]>;
    read5(facts: {
        value1: string;
        value2: string;
    }): Promise<{
        record_id: string;
        action: string;
        actor: string;
        at: number;
    }[]>;
    access(actor: WorkContext): Promise<CalendarAccess>;
};
export function createCalendarIndex(ports: {
    clock: Clock;
    error(status: number, code: string, message: string): Error;
    fail(message: string, code?: string, status?: number): never;
    log(...values: unknown[]): void;
}) {
    async function calendarData(db: CalendarIndexUnit, actor: WorkContext, raw: unknown): Promise<CalendarData> {
        const q = createCalendarQuerySchema(()=>calendarTodayAt(new Date(ports.clock.now()))).parse(raw), a = await db.access(actor);
        if (q.preset)
            q.mode = q.preset === "mine" ? "mine" : "team";
        q.mode ??= a.teamDefault ? "team" : "mine";
        if (q.node) {
            if (!a.scopeIds.includes(q.node))
                ports.fail("Choose a unit in this workspace.");
            const selected = descendants(a.nodes, q.node);
            a.scopeIds = a.scopeIds.filter((id) => selected.includes(id));
            a.rosterIds = a.rosterIds.filter((id) => selected.includes(id));
        }
        if (q.person &&
            !(await db.read1({ value1: q.person, value2: actor.companyId, value3: idList(a.peopleIds) })))
            ports.fail("This person is outside your calendar people scope.");
        const { from, to } = calendarRange(q);
        const bounds = await db.read2({ value1: from, value2: q.timezone, value3: to });
        const take = q.cursor + q.limit + 1, sources: CalendarData["sources"] = [];
        const adapters = Object.fromEntries(calendarLayers.map(layer => [layer, () => db.layer(layer, { actor, access: a, query: q, from, to, fromMs: bounds!.from_ms, toMs: bounds!.to_ms, take })])) as Record<CalendarLayer, () => Promise<CalendarItem[]>>;
        const selected = calendarLayers.filter((layer) => q.layers.split(",").includes(layer));
        const responses = await Promise.allSettled(selected.map((layer) => adapters[layer]()));
        const all: CalendarItem[] = [];
        for (let i = 0; i < responses.length; i++) {
            const response = responses[i], source = selected[i];
            if (response.status === "fulfilled") {
                sources.push({ source, status: "ready" });
                all.push(...response.value);
            }
            else {
                sources.push({
                    source,
                    status: "error",
                    message: `${source[0].toUpperCase() + source.slice(1)} could not be loaded. Refresh to retry.`,
                });
                ports.log("Calendar source failed", source, response.reason instanceof Error
                    ? response.reason.message
                    : "Unknown error");
            }
        }
        // Sorting by stable source identity makes pagination independent of the presentation/hierarchy.
        const unique = [...new Map(all.map((item) => [item.id, item])).values()].sort((x, y) => x.id.localeCompare(y.id));
        const items = unique.slice(q.cursor, q.cursor + q.limit), nextCursor = unique.length > q.cursor + q.limit ? q.cursor + q.limit : null;
        for (const item of items) {
            if (item.source === "appointments" &&
                !item.redacted &&
                item.status === "scheduled")
                item.warnings.push(...(await db.warnings(actor.companyId, item.participants.map((p) => p.id), item.startsAt!, item.endsAt!, item.timezone, item.sourceId, item.details?.attendees?.map(attendee => attendee.userId) ?? [])));
            if (item.source === "roster") {
                const pp = item.participants.filter((p) => a.availability || p.id === a.staff?.person_id);
                if (pp.length &&
                    (await db.read3({ value1: actor.companyId, value2: idList(pp.map((p) => p.id)), value3: item.endsAt!, value4: item.timezone, value5: item.startsAt! })))
                    item.warnings.push("Assigned staff have approved absence during this shift.");
            }
        }
        const legacyIds = items
            .filter((x) => !x.redacted && ["roster", "leave", "lifecycle"].includes(x.source))
            .map((x) => x.sourceId);
        if (legacyIds.length) {
            const history = await db.read4({ value1: actor.companyId, value2: idList(legacyIds) });
            for (const item of items)
                if (!item.redacted)
                    item.history.push(...history
                        .filter((h) => h.record_id === item.sourceId)
                        .map((h) => ({ action: h.action, actor: h.actor, at: h.at })));
        }
        const historyIds = items
            .filter((x) => !x.redacted && ["appointment", "announcement", "task"].includes(x.kind))
            .map((x) => x.sourceId);
        if (historyIds.length) {
            const history = await db.read5({ value1: actor.companyId, value2: idList(historyIds) });
            for (const item of items)
                if (!item.redacted)
                    item.history.push(...history
                        .filter((h) => h.record_id === item.sourceId)
                        .map((h) => ({ action: h.action, actor: h.actor, at: h.at })));
        }
        const shifts = items.filter((x) => x.source === "roster");
        return {
            query: q,
            from,
            to,
            items,
            nextCursor,
            sources,
            coverage: {
                shifts: shifts.length,
                assigned: shifts.reduce((n, s) => n + (s.assigned ?? 0), 0),
                open: shifts.reduce((n, s) => n + Math.max(0, (s.capacity ?? 0) - (s.assigned ?? 0)), 0),
                warnings: items.filter((s) => s.warnings.length).length,
                partial: nextCursor !== null ||
                    q.cursor > 0 ||
                    sources.some((s) => s.status === "error"),
            },
            selfPersonId: a.staff?.person_id ?? null,
            canManageRoster: hasPermission(actor, "roster.manage"),
            canSchedule: a.manualPurchase?.newWork??true,
            canAnnounce: hasPermission(actor, "calendar.announcements.publish")&&(a.manualPurchase?.newWork??true),
            teamDefault: a.teamDefault,
            ...(a.manualPurchase?{purchase:a.manualPurchase}:{}),
        };
    }
    return { calendarData };
}
