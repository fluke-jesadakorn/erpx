import { bangkokToday, handoverKind, type HandoverItem } from "../domain/handover-time";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { WorkSnapshot } from "@/server/modules/work_projection/public/work-items";
import type { readDailyPlan } from "@/server/modules/work_projection/public/daily-plan";
import type { publishPost } from "@/server/modules/collaboration/public/social-home";
import type { savePostDraft } from "@/server/modules/collaboration/public/social-home";
import type { readablePost } from "@/server/modules/collaboration/public/social-source";
import type { sourceGuard } from "@/server/modules/collaboration/public/social-source";
import type { staffPeople } from "@/server/modules/operations/public/staff-policy";
import type { getTask } from "@/server/modules/operations/public/staff-tasks";
import type { readablePmsSource } from "@/server/modules/pms/public/pms/source";
import type { pmsSourceRecipients } from "@/server/modules/pms/public/pms/source";
import { type HandoverView, handoverPublishSchema, peopleInScope, type HandoverAcknowledgement } from '../domain/handover';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import type { OperationsCommands } from '../domain/commands';
type Tail<Value extends unknown[]> = Value extends [
    unknown,
    ...infer Rest
] ? Rest : never;
type PortResult<Value, Effect> = Value extends Promise<infer Result> ? Promise<PortResult<Result, Effect>> : Value extends {
    bind(...args: never[]): unknown;
    run(): unknown;
} ? Effect : Value extends (...args: never[]) => unknown ? Value : Value extends readonly unknown[] ? {
    [Index in keyof Value]: PortResult<Value[Index], Effect>;
} : Value extends object ? {
    [Key in keyof Value]: PortResult<Value[Key], Effect>;
} : Value;
export interface HandoverRepository<Effect> {
    readHandoverViewVersions(companyId: string, ids: string[]): Promise<{
        results: ({
            id: string;
            version: number;
        })[];
    }>;
    readPublishHandoverExisting(value: string, companyId: string, userId: string): Promise<{
        body: string;
        participants_json: string;
        sources_json: string;
        published_post_id: string | null;
    } | null>;
    readPublishHandoverAccessible(recipientId: string, id: string, companyId: string): Promise<Record<string, unknown> | null>;
    readPublishHandoverAccessible2(recipientId: string, id: string, companyId: string): Promise<Record<string, unknown> | null>;
    readHandoverAcknowledgementRecipients(postId: string, author_id: string): Promise<{
        results: ({
            user_id: string;
        })[];
    }>;
    readHandoverAcknowledgementAcknowledgements(postId: string, version: number, ids: string[]): Promise<{
        results: ({
            user_id: string;
        })[];
    }>;
    guard7(key: string, value: "handover-recipient", postId: string, userId: string, postId2: string, userId2: string, version: number): Effect;
    stageAcknowledgeHandoverEffect(postId: string, userId: string, version: number, now: number, key: string): Effect;
}
export interface HandoverPorts<Effect> {
    repository: HandoverRepository<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    commands: OperationsCommands<Effect>;
    prepare(holdingId: string): Promise<void>;
    production: boolean;
    batch(effects: Effect[]): Promise<unknown>;
    policyRead<Result>(key: string, read: () => Promise<Result>): Promise<Result>;
    policyNow(): number;
    readDailyPlan(...args: Tail<Parameters<typeof readDailyPlan>>): PortResult<ReturnType<typeof readDailyPlan>, Effect>;
    publishPost(...args: Tail<Parameters<typeof publishPost>>): PortResult<ReturnType<typeof publishPost>, Effect>;
    savePostDraft(...args: Tail<Parameters<typeof savePostDraft>>): PortResult<ReturnType<typeof savePostDraft>, Effect>;
    readablePost(...args: Tail<Parameters<typeof readablePost>>): PortResult<ReturnType<typeof readablePost>, Effect>;
    sourceGuard(...args: Tail<Parameters<typeof sourceGuard>>): PortResult<ReturnType<typeof sourceGuard>, Effect>;
    staffPeople(...args: Tail<Parameters<typeof staffPeople>>): PortResult<ReturnType<typeof staffPeople>, Effect>;
    getTask(...args: Tail<Parameters<typeof getTask>>): PortResult<ReturnType<typeof getTask>, Effect>;
    readablePmsSource(...args: Tail<Parameters<typeof readablePmsSource>>): PortResult<ReturnType<typeof readablePmsSource>, Effect>;
    pmsSourceRecipients(...args: Tail<Parameters<typeof pmsSourceRecipients>>): PortResult<ReturnType<typeof pmsSourceRecipients>, Effect>;
}
export function createHandoverApplication<Effect>(ports: HandoverPorts<Effect>) {
    const r = ports.repository;
    async function handoverView(actor: WorkContext, snapshot: WorkSnapshot, now = ports.clock.now()): Promise<HandoverView> {
        const date = bangkokToday(now);
        const tasks = snapshot.items.filter(item => ["staff_task", "pms"].includes(item.type) && item.status !== "cancelled" && item.status !== "draft");
        const [plan, people, pmsPeople] = await Promise.all([
            ports.readDailyPlan(actor, date, snapshot), ports.staffPeople(actor.companyId), ports.pmsSourceRecipients(actor),
        ]);
        const planned = new Set(plan.items.filter(item => item.type === "staff_task").map(item => item.id));
        const ids = tasks.filter(item => item.type === "staff_task").map(item => item.id), versions = ids.length ? (await r.readHandoverViewVersions(actor.companyId, ids)).results : [];
        const byId = new Map(versions.map(item => [item.id, Number(item.version)]));
        for (const item of tasks.filter(item => item.type === "pms")) {
            try {
                const source = await ports.readablePmsSource(actor, item.id);
                byId.set(item.id, Number(source.version));
            }
            catch (cause) {
                if (!(cause instanceof BusinessRuleError && cause.status === 404))
                    throw cause;
            }
        }
        const midnight = Date.parse(`${date}T00:00:00Z`) - 7 * 60 * 60000;
        const items: HandoverItem[] = tasks.flatMap(item => {
            const sourceVersion = byId.get(item.id);
            if (!sourceVersion)
                return [];
            const kind = handoverKind(item);
            if (kind === "completed" && item.updatedAt < midnight && !planned.has(item.id))
                return [];
            return [{ id: item.id, title: item.title, status: item.status, updatedAt: item.updatedAt, href: item.href,
                    nextAction: item.nextAction, blocker: item.blocker, kind, planned: planned.has(item.id), sourceVersion,
                    sourceKind: item.type === "pms" ? "pms_record" as const : "staff_task" as const }];
        }).sort((a, b) => Number(b.planned) - Number(a.planned) ||
            Number(b.kind === "blocked") - Number(a.kind === "blocked") || b.updatedAt - a.updatedAt).slice(0, 30);
        const unavailable = items.length === 0;
        const suggested = items.filter(item => item.planned || item.kind === "blocked" || item.kind === "completed");
        return { date, ownerKey: JSON.stringify([actor.companyId, actor.userId]), items, defaultIds: (suggested.length ? suggested : items.slice(0, 3))
                .slice(0, 5).map(item => item.id), people: [...new Map([...peopleInScope(actor, people).map(person => ({ id: person.user_id, name: person.legal_name })), ...pmsPeople].map(person => [person.id, person])).values()],
            unavailable, reason: unavailable && snapshot.unavailableSources.includes("Tasks") ? "Tasks are temporarily unavailable. Try again when they return."
                : items.length === 0 ? "No readable task work is available for a handover yet." : null };
    }
    async function publishHandover(actor: WorkContext, snapshot: WorkSnapshot, raw: unknown, key: string) {
        const input = handoverPublishSchema.parse(raw);
        if (input.date !== bangkokToday(ports.clock.now()))
            throw new BusinessRuleError(409, "HANDOVER_DATE_CHANGED", "Refresh your handover for today.");
        const existing = await r.readPublishHandoverExisting(`post-draft-${key}`, actor.companyId, actor.userId);
        const sources = input.sources.map(item => ({ kind: item.kind, id: item.id, ...(item.kind === "pms_record" ? { version: item.sourceVersion } : {}) }));
        const recipients = [...input.recipientIds].sort();
        if (existing?.published_post_id) {
            await ports.readablePost(actor, existing.published_post_id);
            const storedRecipients = (JSON.parse(existing.participants_json) as string[]).sort();
            const storedSources = JSON.parse(existing.sources_json) as typeof sources;
            if (existing.body !== input.body || JSON.stringify(storedRecipients) !== JSON.stringify(recipients) ||
                JSON.stringify(storedSources) !== JSON.stringify(sources))
                throw new BusinessRuleError(409, "IDEMPOTENCY_CONFLICT", "This publish key was already used for another handover.");
            return { id: existing.published_post_id, version: 1 };
        }
        if (input.sources.some(source => source.kind === "staff_task") && snapshot.unavailableSources.includes("Tasks"))
            throw new BusinessRuleError(503, "HANDOVER_SOURCE_UNAVAILABLE", "Tasks are temporarily unavailable.");
        const visible = new Set(snapshot.items.filter(item => item.type === "staff_task").map(item => item.id));
        const pmsOnly = input.sources.every(source => source.kind === "pms_record");
        const eligible = new Set(pmsOnly ? (await ports.pmsSourceRecipients(actor)).map(person => person.id)
            : [...peopleInScope(actor, await ports.staffPeople(actor.companyId)).map(person => person.user_id), ...(await ports.pmsSourceRecipients(actor)).map(person => person.id)]);
        if (recipients.some(id => !eligible.has(id)))
            throw new BusinessRuleError(403, "HANDOVER_RECIPIENT_UNAVAILABLE", "Choose current colleagues within your work scope.");
        for (const source of input.sources) {
            if (source.kind === "pms_record") {
                const item = await ports.readablePmsSource(actor, source.id);
                if (Number(item.version) !== source.sourceVersion)
                    throw new BusinessRuleError(409, "HANDOVER_SOURCE_CHANGED", "Selected hotel work changed. Refresh before publishing.");
                for (const recipientId of recipients) {
                    const accessible = await r.readPublishHandoverAccessible(recipientId, source.id, actor.companyId);
                    if (!accessible)
                        throw new BusinessRuleError(403, "HANDOVER_RECIPIENT_ACCESS", "A colleague cannot open every linked hotel item. Choose another recipient or fewer items.");
                }
                continue;
            }
            if (!visible.has(source.id))
                throw new BusinessRuleError(403, "HANDOVER_SOURCE_UNAVAILABLE", "A selected task is no longer available to you.");
            const task = await ports.getTask(actor, source.id);
            if (Number(task.version) !== source.sourceVersion)
                throw new BusinessRuleError(409, "HANDOVER_SOURCE_CHANGED", "A selected task changed. Refresh the handover before publishing.");
            if (task.source_post_id)
                throw new BusinessRuleError(403, "HANDOVER_SOURCE_PRIVATE", "This task comes from a conversation and cannot be included in a handover.");
            for (const recipientId of recipients) {
                const accessible = await r.readPublishHandoverAccessible2(recipientId, source.id, actor.companyId);
                if (!accessible)
                    throw new BusinessRuleError(403, "HANDOVER_RECIPIENT_ACCESS", "A selected colleague cannot open every linked task. Choose another recipient or fewer tasks.");
            }
        }
        const draft = await ports.savePostDraft(actor, null, { body: input.body, content: [{ type: "text", text: input.body }],
            mentions: [], audience: { kind: "participants", userIds: recipients }, sources, purpose: "handover", expectedVersion: 1 }, key);
        return ports.publishPost(actor, draft.id, draft.version, `${key}-publish`);
    }
    async function handoverAcknowledgement(actor: WorkContext, postId: string): Promise<HandoverAcknowledgement> {
        const post = await ports.readablePost(actor, postId);
        if (post.purpose !== "handover" || post.audience_kind !== "participants")
            throw new BusinessRuleError(404, "HANDOVER_NOT_FOUND", "This handover is unavailable.");
        const recipients = (await r.readHandoverAcknowledgementRecipients(postId, post.author_id)).results;
        const ids = recipients.map(person => person.user_id);
        const acknowledgements = ids.length ? (await r.readHandoverAcknowledgementAcknowledgements(postId, post.version, ids)).results : [];
        const canAcknowledge = ids.includes(actor.userId);
        return { postId, postVersion: post.version, canAcknowledge, acknowledged: canAcknowledge && acknowledgements.some(item => item.user_id === actor.userId),
            recipientCount: ids.length, acknowledgedCount: acknowledgements.length };
    }
    async function acknowledgeHandover(actor: WorkContext, postId: string, key: string) {
        const post = await ports.readablePost(actor, postId);
        const state = await handoverAcknowledgement(actor, postId);
        if (!state.canAcknowledge)
            throw new BusinessRuleError(403, "HANDOVER_RECIPIENT_REQUIRED", "Only an intended recipient can confirm this handover.");
        if (state.acknowledged)
            return state;
        const now = ports.clock.now();
        await ports.commands.commit(actor, key, "social.handover.acknowledge", { postId, postVersion: post.version }, postId, post.version, { postId, postVersion: post.version, acknowledged: true }, [
            ports.sourceGuard(key, post, actor.userId, "handover-readable"),
            r.guard7(key, "handover-recipient", postId, actor.userId, postId, actor.userId, post.version),
            r.stageAcknowledgeHandoverEffect(postId, actor.userId, post.version, now, key),
        ], { companyId: actor.companyId, branchId: post.branch_id, departmentId: post.department_id });
        return handoverAcknowledgement(actor, postId);
    }
    return { handoverView, publishHandover, handoverAcknowledgement, acknowledgeHandover };
}
