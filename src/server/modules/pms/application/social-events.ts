import type { EventMessage } from "../../workflow/public/job-projector";
import { decodePmsSource, type PmsSource } from "../domain/source";
import { pmsSocialEvents } from "../domain/social-events";
import type { Clock } from "../../shared/application/ports";
export type PmsSocialOriginal = {
    command_id: string;
    created_at: number;
    actor_id: string;
    result_json: string;
};
export type PmsSocialPost = {
    postId: string;
    companyId: string;
    branchId: string;
    actorId: string;
    body: string;
    commandId: string;
    at: number;
    sourceId: string;
    eventId: string;
    receivedAt: number;
};
export type PmsSocialNotice = {
    id: string;
    companyId: string;
    userId: string;
    postId: string;
    version: number | null;
    body: string;
    at: number;
};
export interface PmsSocialUnit {
    original(id: string, eventType: string): Promise<PmsSocialOriginal | null>;
    payoutSource(id: string, propertyId: string): Promise<string | null>;
    source(id: string | null, companyId: string, propertyId: string): Promise<PmsSource | null>;
    lock(postId: string): Promise<unknown>;
    received(eventId: string): Promise<unknown>;
    publish(post: PmsSocialPost): Promise<void>;
    recipients(id: string, companyId: string, actorId: string, review: string | null, recipient: string | null): Promise<Array<{
        user_id: string;
    }>>;
    version(postId: string): Promise<number | null>;
    notify(notice: PmsSocialNotice): Promise<unknown>;
}
export function createPmsSocialProjector(ports: {
    transaction<Result>(work: (unit: PmsSocialUnit) => Promise<Result>): Promise<Result>;
    clock: Clock;
    hash(value: string): string;
}) {
    async function projectPmsSocialEvent(message: EventMessage) {
        const event = pmsSocialEvents[message.eventType];
        if (!event)
            return;
        await ports.transaction(async (tx) => {
            const original = await tx.original(message.id, message.eventType);
            if (!original)
                return;
            const result = JSON.parse(original.result_json) as {
                id?: string;
                propertyId?: string;
            };
            const sourceId = message.eventType === "pms.refund.pay" ? await tx.payoutSource(result.id ?? "", result.propertyId ?? message.aggregateId) : result.id ?? message.aggregateId;
            const row = await tx.source(sourceId, message.payload.companyId, result.propertyId ?? message.aggregateId);
            if (!row)
                return;
            const source = decodePmsSource(row);
            if (!["reservation", "stay", "task", "transfer", "correction", "refund", "shift", "night"].includes(source.kind))
                return;
            const day = new Date(Number(original.created_at) + 7 * 60 * 60000).toISOString().slice(0, 10), postId = "pms-post-" + ports.hash(JSON.stringify([source.company_id, source.id, day, event.group])).slice(0, 40);
            await tx.lock(postId);
            if (await tx.received(message.id))
                return;
            const body = `${source.property_name} · ${event.label}\n${source.data.state ?? "updated"} · วันที่ธุรกิจ / Business date ${source.data.date ?? source.business_date}`;
            await tx.publish({ postId, companyId: source.company_id, branchId: source.branch_id, actorId: original.actor_id, body, commandId: original.command_id, at: Number(original.created_at), sourceId: source.id, eventId: message.id, receivedAt: ports.clock.now() });
            const review = message.eventType === "pms.payment.request_transfer" ? "finance.approve" : ["pms.refund.request", "pms.charge.correction.request"].includes(message.eventType) ? "pms.approve" : message.eventType === "pms.task.complete" ? "pms.inspect" : null;
            const recipient = source.data.recordedBy ?? source.data.requestedBy ?? source.data.assigneeId ?? source.data.userId ?? null;
            const recipients = await tx.recipients(source.id, source.company_id, original.actor_id, review, recipient), version = await tx.version(postId);
            for (const recipient of recipients)
                await tx.notify({ id: "pms-notice-" + ports.hash(JSON.stringify([postId, recipient.user_id])).slice(0, 40), companyId: source.company_id, userId: recipient.user_id, postId, version, body, at: Number(original.created_at) });
        });
    }
    return { projectPmsSocialEvent };
}
