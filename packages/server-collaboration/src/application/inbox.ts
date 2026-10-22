import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { sourceRecordLabel, type SourceRecordReference } from "@erpx/feature-records/lib/source-record";
import { staffNotificationSourceTypes, type StaffNotification } from '@erpx/server-collaboration/domain/staff-notifications';
import { noticeInput, type InboxCategory, type InboxEventInput, type InboxItem, type Candidate } from '@erpx/server-collaboration/domain/inbox';
export interface InboxRepository {
    publishEvent(id: string, companyId: string, key: string, sourceType: string, sourceId: string, sourceVersion: number, value: string | null, category: InboxCategory, preview: string, now: number): Promise<unknown>;
    publishedEvent(companyId: string, key: string): Promise<{
        id: string;
    } | null>;
    deliverEvent(value: string, id: string, companyId: string, userId: string, now: number): Promise<unknown>;
    noticeExists(source_id: string, companyId: string): Promise<Record<string, unknown> | null>;
    calendarAppointment(source_id: string, companyId: string): Promise<{
        id: string;
        version: number;
    } | null>;
    calendarRecipient(source_id: string, companyId: string, userId: string, source_id2: string, companyId2: string, userId2: string): Promise<Record<string, unknown> | null>;
    candidate(id: string, companyId: string, userId: string): Promise<Candidate | null>;
    changeDeliveryState(readAt: number | null, archivedAt: number | null, id: string, companyId: string, userId: string): Promise<unknown>;
    changeLegacyState(readAt: number | null, id: string, companyId: string, userId: string): Promise<unknown>;
    broadcastPermission(role: string, companyId: string, value: "inbox.notice.broadcast"): Promise<Record<string, unknown> | null>;
    audienceNode(id: string, companyId: string): Promise<{
        id: string;
        kind: string;
        branch_id: string | null;
    } | null>;
    recentNoticeCount(companyId: string, userId: string, value: number): Promise<{
        n: number;
    } | null>;
    audienceRecipients(companyId: string, now: number, now2: number, kind: "company" | "branch" | "department", kind2: "company" | "branch" | "department", id: string, id2: string, kind3: "company" | "branch" | "department", id3: string, id4: string): Promise<{
        results: {
            user_id: string;
        }[];
    }>;
    activeRecipients(companyId: string, ids: string[], now: number, now2: number): Promise<{
        results: {
            user_id: string;
        }[];
    }>;
    insertNotice(id: string, companyId: string, userId: string, title: string, body: string, value: string, length: number, now: number): Promise<unknown>;
    targetNotice(id: string, companyId: string, userId: string): Promise<unknown>;
    pendingTargets(): Promise<{
        results: {
            notice_id: string;
            company_id: string;
            user_id: string;
        }[];
    }>;
    activeFanoutRecipient(company_id: string, user_id: string, now: number, now2: number): Promise<Record<string, unknown> | null>;
    noticeEvent(company_id: string, value: string): Promise<{
        id: string;
    } | null>;
    deliverNotice(value: string, id: string, company_id: string, user_id: string, now: number): Promise<unknown>;
    completeNoticeTarget(now: number, notice_id: string, user_id: string): Promise<unknown>;
    pendingMetrics(): Promise<{
        count: number;
        oldest: number | null;
    } | null>;
    unresolvedMetrics(types: string[]): Promise<{
        count: number;
    } | null>;
    failedMetrics(): Promise<{
        count: number;
    } | null>;
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface InboxPorts<Effect> {
    repository: InboxRepository;
    commands: OwnerCommands<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    candidates(actor: WorkContext, view: string | undefined, category: string | null, search: string, cursor: {
        at: number;
        id: string;
    } | null): Promise<{
        results: Candidate[];
    }>;
    visibleNotifications(actor: WorkContext, rows: Omit<StaffNotification, 'href'>[]): Promise<StaffNotification[]>;
}
export function createInboxApplication<Effect>(ports: InboxPorts<Effect>) {
    const r = ports.repository;
    async function publishInboxEvent(input: InboxEventInput) {
        const now = input.createdAt ?? ports.clock.now();
        const id = ports.identifiers.next();
        await r.publishEvent(id, input.companyId, input.key, input.sourceType, input.sourceId, input.sourceVersion, input.actorUserId ?? null, input.category, input.preview, now);
        const event = await r.publishedEvent(input.companyId, input.key);
        if (!event)
            throw new Error("INBOX_EVENT_MISSING");
        for (const userId of new Set(input.recipientIds)) {
            await r.deliverEvent(ports.identifiers.next(), event.id, input.companyId, userId, now);
        }
        return event.id;
    }
    async function resolveCandidate(actor: WorkContext, row: Candidate): Promise<InboxItem | null> {
        let href: string | null = null;
        let source: SourceRecordReference | undefined;
        if (row.source_type === "notice") {
            const notice = await r.noticeExists(row.source_id, actor.companyId);
            if (!notice)
                return null;
        }
        else if (row.source_type === "calendar_appointment" || row.source_type === "calendar_removed") {
            const appointment = await r.calendarAppointment(row.source_id, actor.companyId);
            if (!appointment)
                return null;
            if (row.source_type === "calendar_appointment") {
                const invited = await r.calendarRecipient(row.source_id, actor.companyId, actor.userId, row.source_id, actor.companyId, actor.userId);
                if (!invited)
                    return null;
                href = `/calendar?event=appointment:${encodeURIComponent(row.source_id)}`;
                source = { kind: row.source_type, id: appointment.id, version: appointment.version, href, label: sourceRecordLabel(row.source_type) };
            }
        }
        else {
            if (!(staffNotificationSourceTypes as readonly string[]).includes(row.source_type))
                return null;
            const legacy: Omit<StaffNotification, "href"> = { id: row.id, record_type: row.source_type, record_id: row.source_id,
                record_version: row.source_version, message: row.preview, created_at: row.delivered_at, read_at: row.read_at };
            const visible = await ports.visibleNotifications(actor, [legacy]);
            if (!visible.length)
                return null;
            href = visible[0].href;
            source = visible[0].source;
        }
        return { id: row.id, eventId: row.event_id, category: row.category, message: row.preview, href,
            createdAt: row.delivered_at, readAt: row.read_at, archivedAt: row.archived_at,
            sourceType: row.source_type, sourceId: row.source_id, source };
    }
    function cursorDecode(value?: string) {
        if (!value)
            return null;
        try {
            if (value.length > 512)
                throw Error();
            const v = JSON.parse(Buffer.from(value, "base64url").toString()) as {
                at: number;
                id: string;
            };
            if (!Number.isSafeInteger(v.at) || typeof v.id !== "string" || v.id.length > 180)
                throw Error();
            return v;
        }
        catch {
            throw new BusinessRuleError(422, "INVALID_CURSOR", "Restart the Inbox list.");
        }
    }
    function cursorEncode(item: InboxItem) { return Buffer.from(JSON.stringify({ at: item.createdAt, id: item.id })).toString("base64url"); }
    async function inboxPage(actor: WorkContext, query: {
        view?: string;
        category?: string;
        search?: string;
        cursor?: string;
        limit?: number;
    }) {
        const limit = Math.max(1, Math.min(100, query.limit ?? 30));
        const category = query.category && ["work", "calendar", "mention", "notice", "system"].includes(query.category) ? query.category : null;
        const search = (query.search ?? "").trim().slice(0, 100);
        let cursor = cursorDecode(query.cursor), exhausted = false;
        const items: InboxItem[] = [];
        while (!exhausted && items.length <= limit) {
            const rows = (await ports.candidates(actor, query.view, category, search, cursor)).results;
            if (rows.length < 100)
                exhausted = true;
            if (!rows.length)
                break;
            for (const row of rows) {
                cursor = { at: row.delivered_at, id: row.id };
                const resolved = await resolveCandidate(actor, row);
                if (resolved)
                    items.push(resolved);
                if (items.length > limit)
                    break;
            }
        }
        const hasMore = items.length > limit;
        const page = items.slice(0, limit);
        return { items: page, nextCursor: hasMore ? cursorEncode(page[page.length - 1]) : null, hasMore };
    }
    async function inboxSummary(actor: WorkContext) {
        const counts: Record<InboxCategory, number> = { work: 0, calendar: 0, mention: 0, notice: 0, system: 0 };
        let cursor: string | undefined;
        for (;;) {
            const page = await inboxPage(actor, { view: "unread", cursor, limit: 100 });
            for (const item of page.items)
                counts[item.category]++;
            if (!page.nextCursor)
                break;
            cursor = page.nextCursor;
        }
        return { unread: Object.values(counts).reduce((a, b) => a + b, 0), categories: counts };
    }
    async function changeInboxState(actor: WorkContext, id: string, state: {
        read?: boolean;
        archived?: boolean;
    }) {
        const row = await r.candidate(id, actor.companyId, actor.userId);
        if (!row || !await resolveCandidate(actor, row))
            throw new BusinessRuleError(404, "INBOX_ITEM_NOT_FOUND", "This update is unavailable.");
        const readAt = state.read === undefined ? row.read_at : state.read ? (row.read_at ?? ports.clock.now()) : null;
        const archivedAt = state.archived === undefined ? row.archived_at : state.archived ? (row.archived_at ?? ports.clock.now()) : null;
        await r.changeDeliveryState(readAt, archivedAt, id, actor.companyId, actor.userId);
        if (row.event_id.startsWith("legacy:"))
            await r.changeLegacyState(readAt, id, actor.companyId, actor.userId);
        return { id, readAt, archivedAt };
    }
    async function sendPreparedNotice(actor: WorkContext, input: z.infer<typeof noticeInput>, key: string) {
        const id = `notice-${key}`, operation = "inbox.notice.send";
        const replay = await ports.commands.replay<{
            id: string;
            recipientCount: number;
        }>(actor, key, operation, id, input);
        if (replay)
            return replay;
        const now = ports.clock.now();
        if (input.audience) {
            const allowed = hasPermission(actor, "inbox.notice.broadcast") && await r.broadcastPermission(actor.role, actor.companyId, "inbox.notice.broadcast");
            if (!allowed)
                throw new BusinessRuleError(403, "BROADCAST_FORBIDDEN", "Your role cannot broadcast notices.");
            const node = await r.audienceNode(input.audience.id, actor.companyId);
            if (!node || node.kind !== input.audience.kind ||
                input.audience.kind === "company" && input.audience.id !== actor.companyId ||
                actor.departmentId && input.audience.id !== actor.departmentId ||
                actor.branchId && input.audience.kind === "company" ||
                actor.branchId && input.audience.kind === "branch" && input.audience.id !== actor.branchId ||
                actor.branchId && input.audience.kind === "department" && node.branch_id !== actor.branchId)
                throw new BusinessRuleError(403, "BROADCAST_SCOPE", "Choose an audience inside your current scope.");
        }
        const recent = await r.recentNoticeCount(actor.companyId, actor.userId, now - 3600000);
        if ((recent?.n ?? 0) >= 20)
            throw new BusinessRuleError(429, "NOTICE_RATE_LIMIT", "Try again after your notice limit resets.");
        let ids = input.recipients;
        if (input.audience) {
            const audience = input.audience;
            ids = (await r.audienceRecipients(actor.companyId, now, now, audience.kind, audience.kind, audience.id, audience.id, audience.kind, audience.id, audience.id)).results.map(row => row.user_id);
            if (ids.length > 10000)
                throw new BusinessRuleError(422, "AUDIENCE_TOO_LARGE", "Choose a smaller audience.");
        }
        ids = [...new Set(ids.filter(value => value !== actor.userId))];
        if (!ids.length)
            throw new BusinessRuleError(422, "NO_RECIPIENTS", "Choose at least one colleague.");
        const active = (await r.activeRecipients(actor.companyId, ids, now, now)).results.map(row => row.user_id);
        if (active.length !== ids.length)
            throw new BusinessRuleError(422, "RECIPIENT_UNAVAILABLE", "Choose active accounts in this Company.");
        const result = { id, recipientCount: ids.length };
        await r.insertNotice(id, actor.companyId, actor.userId, input.title, input.body, JSON.stringify(input.audience ?? { kind: "named", ids }), ids.length, now);
        await publishInboxEvent({ key: `notice:${id}`, companyId: actor.companyId, sourceType: "notice", sourceId: id,
            sourceVersion: 1, actorUserId: actor.userId, category: "notice", preview: `${input.title} — ${input.body}`, recipientIds: input.audience ? [] : ids, createdAt: now });
        if (input.audience)
            for (const userId of ids)
                await r.targetNotice(id, actor.companyId, userId);
        await ports.commands.commit(actor, key, operation, input, id, 1, result, []);
        return result;
    }
    async function drainInboxNotices() {
        const targets = (await r.pendingTargets()).results;
        for (const target of targets) {
            const active = await r.activeFanoutRecipient(target.company_id, target.user_id, ports.clock.now(), ports.clock.now());
            if (active) {
                const event = await r.noticeEvent(target.company_id, `notice:${target.notice_id}`);
                if (event)
                    await r.deliverNotice(ports.identifiers.next(), event.id, target.company_id, target.user_id, ports.clock.now());
            }
            await r.completeNoticeTarget(ports.clock.now(), target.notice_id, target.user_id);
        }
        return targets.length;
    }
    async function inboxOperationalMetrics(now = ports.clock.now()) {
        const pending = await r.pendingMetrics();
        const types = ["notice", "calendar_appointment", "calendar_removed", ...staffNotificationSourceTypes];
        const unresolved = await r.unresolvedMetrics(types);
        const failed = await r.failedMetrics();
        return { pendingDeliveries: pending?.count ?? 0, oldestDeliveryLagMs: pending?.oldest ? now - pending.oldest : 0,
            unresolvedEventTypes: unresolved?.count ?? 0, failedJobs: failed?.count ?? 0 };
    }
    return { publishInboxEvent, inboxPage, inboxSummary, changeInboxState, sendPreparedNotice, drainInboxNotices, inboxOperationalMetrics };
}
export function createInboxNoticeApplication(ports: {
    execute(actor: WorkContext, input: z.infer<typeof noticeInput>, key: string): Promise<{
        id: string;
        recipientCount: number;
    }>;
}) { return { sendNotice(actor: WorkContext, raw: unknown, key: string) { return ports.execute(actor, noticeInput.parse(raw), key); } }; }
