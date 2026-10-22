import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import { sourceType, reason, relationSchema, cardSchema, actionSchema, csvCell, type Row, type Source, type Card } from '@erpx/server-collaboration/domain/collaboration-work';
export interface CollaborationWorkRepository<Effect> {
    recordRelations(companyId: string): Promise<{
        results: Row[];
    }>;
    actionCards(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<{
        results: Card[];
    }>;
    eligiblePeople(companyId: string): Promise<{
        results: Row[];
    }>;
    actionHistory(id: string): Promise<{
        results: Row[];
    }>;
    prepareRelation(id: string, companyId: string, branchId: string | null, departmentId: string | null, type: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id2: string, version: number, type2: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id3: string, version2: number, relation: "related" | "duplicates" | "blocks" | "depends_on", reason: string, userId: string, now: number, now2: number): Effect;
    prepareRelationEvent(value: string, id: string, userId: string, reason: string, now: number): Effect;
    recordRelation(id: string, companyId: string): Promise<Row | null>;
    guardRelationVersion(key: string, value: "relation", id: string, companyId: string, expectedVersion: number): Effect;
    prepareRelationRemoval(userId: string, now: number, now2: number, id: string): Effect;
    prepareRelationRemovalEvent(value: string, id: string, value2: number, userId: string, why: string, now: number): Effect;
    prepareCard(id: string, companyId: string, branchId: string | null, departmentId: string | null, type: "commerce_order" | "support_ticket" | "field_service_order" | "staff_task" | "pos_sale" | "pms_record" | "asset" | "maintenance_plan" | "maintenance_work", id2: string, version: number, title: string, ownerId: string, dueAt: number, userId: string, now: number, now2: number): Effect;
    guardCardOwner(key: string, value: "owner", id: string): Effect;
    prepareCardEvent(value: string, id: string, userId: string, reason: string, ownerId: string, dueAt: number, now: number): Effect;
    prepareCardNotice(value: string, id: string): Effect;
    actionCard(id: string, companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null): Promise<Card | null>;
    guardCardVersion(key: string, value: "version", id: string, expectedVersion: number): Effect;
    prepareCardTransition(owner: string, due: number, status: "completed" | "cancelled" | "open", evidence: string | null, now: number, id: string): Effect;
    guardReassignedOwner(key: string, value: "owner", id: string): Effect;
    prepareCardTransitionEvent(value: string, id: string, value2: number, action: "complete" | "reassign" | "cancel", userId: string, reason: string, status: "completed" | "cancelled" | "open", owner: string, due: number, now: number): Effect;
    prepareCardTransitionNotice(value: string, id: string): Effect;
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface CollaborationWorkPorts<Effect> {
    repository: CollaborationWorkRepository<Effect>;
    commands: OwnerCommands<Effect>;
    clock: Clock;
    identifiers: IdentifierSource;
    requirePermission(actor: WorkContext, permission: string): WorkContext;
    sha256(value: string): string;
    readSourceRecord(actor: WorkContext, reference: {
        kind: Source["kind"];
        id: string;
    }): Promise<Omit<Source, "type">>;
    sourceRecordGuards(actor: WorkContext, key: string, source: Source, step: string): Promise<Effect[]>;
}
export function createCollaborationWorkApplication<Effect>(ports: CollaborationWorkPorts<Effect>) {
    const r = ports.repository;
    function access(a: WorkContext, write = false) {
        ports.requirePermission(a, write ? "records.write" : "records.read");
        if (!hasModuleAccess(a.modules, "collaboration"))
            throw new BusinessRuleError(403, "MODULE_FORBIDDEN", "Your current role cannot access Collaboration.");
    }
    async function source(a: WorkContext, type: z.infer<typeof sourceType>, id: string): Promise<Source> {
        const record = await ports.readSourceRecord(a, { kind: type, id });
        return { ...record, type, title: record.title };
    }
    async function visibleSource(a: WorkContext, type: z.infer<typeof sourceType>, id: string) {
        try {
            return await source(a, type, id);
        }
        catch (e) {
            if (e instanceof BusinessRuleError && [403, 404].includes(e.status))
                return null;
            throw e;
        }
    }
    async function collaborationWorkspace(a: WorkContext) {
        access(a);
        const relations = (await r.recordRelations(a.companyId)).results, cards = (await r.actionCards(a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId)).results, people = (await r.eligiblePeople(a.companyId)).results;
        const visible = [];
        for (const r of relations) {
            const left = await visibleSource(a, sourceType.parse(r.source_type), String(r.source_id)), right = await visibleSource(a, sourceType.parse(r.target_type), String(r.target_id));
            if (left && right) {
                visible.push(r);
            }
        }
        const visibleCards: Card[] = [];
        for (const card of cards) {
            const record = await visibleSource(a, sourceType.parse(card.source_type), String(card.source_id));
            if (record) visibleCards.push({ ...card, source_title: record.title, source_href: record.href });
        }
        return {
            relations: visible,
            cards: visibleCards,
            people,
            sourceTypes: sourceType.options,
            canWrite: hasPermission(a, "records.write"),
            canReview: hasPermission(a, "records.review"),
        };
    }
    async function collaborationExport(a: WorkContext) {
        const data = await collaborationWorkspace(a), rows: string[][] = [
            [
                "kind",
                "id",
                "status",
                "source_type",
                "source_id",
                "source_title",
                "target_type",
                "target_id",
                "target_title",
                "relation",
                "title",
                "owner_id",
                "owner_name",
                "due_at",
                "version",
                "reason",
            ],
        ];
        for (const relation of data.relations) {
            const left = await visibleSource(a, sourceType.parse(relation.source_type), String(relation.source_id)), right = await visibleSource(a, sourceType.parse(relation.target_type), String(relation.target_id));
            if (!left || !right)
                continue;
            rows.push([
                "relation",
                String(relation.id),
                String(relation.status),
                left.type,
                left.id,
                left.title,
                right.type,
                right.id,
                right.title,
                String(relation.relation),
                "",
                "",
                "",
                "",
                String(relation.version),
                String(relation.reason),
            ]);
        }
        for (const card of data.cards) {
            const linked = await visibleSource(a, sourceType.parse(card.source_type), String(card.source_id));
            if (!linked)
                continue;
            rows.push([
                "action_card",
                String(card.id),
                String(card.status),
                linked.type,
                linked.id,
                linked.title,
                "",
                "",
                "",
                "",
                String(card.title),
                String(card.owner_id),
                String(card.owner_name ?? ""),
                String(card.due_at),
                String(card.version),
                "",
            ]);
        }
        const content = rows.map((row) => row.map(csvCell).join(",")).join("\r\n"), commandId = `collaboration-export-${ports.identifiers.next()}`, result = {
            commandId,
            format: "csv" as const,
            rows: rows.length - 1,
            content,
            sha256: ports.sha256(content),
        };
        return ports.commands.commit(a, commandId, "collaboration.export", { format: "csv" }, commandId, 1, result, []);
    }
    async function collaborationActionDetail(a: WorkContext, id: string) {
        const data = await collaborationWorkspace(a), card = data.cards.find((x) => x.id === id);
        if (!card)
            throw new BusinessRuleError(404, "ACTION_CARD_NOT_FOUND", "Action card not found.");
        return {
            ...data,
            card,
            history: (await r.actionHistory(id)).results,
        };
    }
    async function createRecordRelation(a: WorkContext, raw: unknown, key: string) {
        access(a, true);
        const input = relationSchema.parse(raw), left = await source(a, input.sourceType, input.sourceId), right = await source(a, input.targetType, input.targetId);
        if (left.companyId !== right.companyId || left.companyId !== a.companyId)
            throw new BusinessRuleError(422, "COLLABORATION_SCOPE_MISMATCH", "Both records must belong to the same Company.");
        if (left.type === right.type && left.id === right.id)
            throw new BusinessRuleError(422, "COLLABORATION_SELF_RELATION", "Choose two different records.");
        const symmetric = ["related", "duplicates"].includes(input.relation), swap = symmetric && `${left.type}:${left.id}` > `${right.type}:${right.id}`, from = swap ? right : left, to = swap ? left : right, id = `record-relation-${key}`, op = "collaboration.relation.create", prior = await ports.commands.replay(a, key, op, id, input);
        if (prior)
            return prior;
        const now = ports.clock.now();
        return ports.commands.commit(a, key, op, input, id, 1, { id, version: 1, status: "active" }, [
            ...await ports.sourceRecordGuards(a, key, from, "relation-source"),
            ...await ports.sourceRecordGuards(a, key, to, "relation-target"),
            r.prepareRelation(id, a.companyId, from.branchId, from.departmentId, from.type, from.id, from.version, to.type, to.id, to.version, input.relation, input.reason, a.userId, now, now),
            r.prepareRelationEvent(`record-relation-event-${key}`, id, a.userId, input.reason, now),
        ]);
    }
    async function removeRecordRelation(a: WorkContext, id: string, expectedVersion: number, why: string, key: string) {
        access(a, true);
        reason.parse(why);
        const op = "collaboration.relation.remove", input = { expectedVersion, reason: why }, prior = await ports.commands.replay(a, key, op, id, input);
        const relation = await r.recordRelation(id, a.companyId);
        if (!relation)
            throw new BusinessRuleError(404, "RECORD_RELATION_NOT_FOUND", "The relation is unavailable in this work scope.");
        const left = await visibleSource(a, sourceType.parse(relation.source_type), String(relation.source_id)), right = await visibleSource(a, sourceType.parse(relation.target_type), String(relation.target_id));
        if (!left || !right)
            throw new BusinessRuleError(404, "RECORD_RELATION_NOT_FOUND", "The relation's sources are unavailable in this work scope.");
        if (prior)
            return prior;
        const now = ports.clock.now();
        return ports.commands.commit(a, key, op, input, id, expectedVersion + 1, { id, version: expectedVersion + 1, status: "removed" }, [
            ...await ports.sourceRecordGuards(a, key, left, "relation-source"),
            ...await ports.sourceRecordGuards(a, key, right, "relation-target"),
            r.guardRelationVersion(key, "relation", id, a.companyId, expectedVersion),
            r.prepareRelationRemoval(a.userId, now, now, id),
            r.prepareRelationRemovalEvent(`record-relation-event-${key}`, id, expectedVersion + 1, a.userId, why, now),
        ]);
    }
    async function createActionCard(a: WorkContext, raw: unknown, key: string) {
        access(a, true);
        const input = cardSchema.parse(raw), s = await source(a, input.sourceType, input.sourceId), id = `action-card-${key}`, op = "collaboration.action.create", prior = await ports.commands.replay(a, key, op, id, input);
        if (prior)
            return prior;
        const now = ports.clock.now();
        return ports.commands.commit(a, key, op, input, id, 1, { id, version: 1, status: "open" }, [
            ...await ports.sourceRecordGuards(a, key, s, "action-source"),
            r.prepareCard(id, a.companyId, s.branchId, s.departmentId, s.type, s.id, s.version, input.title, input.ownerId, input.dueAt, a.userId, now, now),
            r.guardCardOwner(key, "owner", id),
            r.prepareCardEvent(`action-card-event-${key}`, id, a.userId, input.reason, input.ownerId, input.dueAt, now),
            r.prepareCardNotice(`action-card-notice-${key}`, id),
        ]);
    }
    async function changeActionCard(a: WorkContext, id: string, raw: unknown, key: string) {
        access(a, true);
        const input = actionSchema.parse(raw), op = `collaboration.action.${input.action}`, prior = await ports.commands.replay(a, key, op, id, input);
        if (prior) {
            await collaborationActionDetail(a, id);
            return prior;
        }
        const row = await r.actionCard(id, a.companyId, a.branchId, a.branchId, a.departmentId, a.departmentId);
        if (!row)
            throw new BusinessRuleError(404, "ACTION_CARD_NOT_FOUND", "Action card not found.");
        const currentSource = await visibleSource(a, sourceType.parse(row.source_type), String(row.source_id));
        if (!currentSource)
            throw new BusinessRuleError(404, "ACTION_CARD_NOT_FOUND", "Action card source is unavailable in this scope.");
        const manage = hasPermission(a, "records.review"), own = row.owner_id === a.userId && Number(row.owner_eligible) > 0;
        if (input.action === "complete" && !own)
            throw new BusinessRuleError(403, "ACTION_CARD_OWNER_REQUIRED", "The assigned owner must complete this action.");
        if (input.action !== "complete" && !manage)
            throw new BusinessRuleError(403, "ACTION_CARD_REVIEW_REQUIRED", "A reviewer must reassign or cancel this action.");
        if (row.status !== "open")
            throw new BusinessRuleError(409, "ACTION_CARD_STATE_CHANGED", "Refresh the action card.");
        const now = ports.clock.now(), status = input.action === "complete"
            ? "completed"
            : input.action === "cancel"
                ? "cancelled"
                : "open", owner = input.action === "reassign" ? input.ownerId : row.owner_id, due = input.action === "reassign" ? input.dueAt : row.due_at, evidence = input.action === "complete" ? input.evidence : null;
        return ports.commands.commit(a, key, op, input, id, input.expectedVersion + 1, { id, version: input.expectedVersion + 1, status }, [
            ...await ports.sourceRecordGuards(a, key, currentSource, "action-source"),
            r.guardCardVersion(key, "version", id, input.expectedVersion),
            r.prepareCardTransition(owner, due, status, evidence, now, id),
            ...(input.action === "reassign"
                ? [
                    r.guardReassignedOwner(key, "owner", id),
                ]
                : []),
            r.prepareCardTransitionEvent(`action-card-event-${key}`, id, input.expectedVersion + 1, input.action, a.userId, input.reason, status, owner, due, now),
            r.prepareCardTransitionNotice(`action-card-notice-${key}`, id),
        ]);
    }
    async function collaborationActionWork(a: WorkContext): Promise<WorkItem[]> {
        if (!hasPermission(a, "records.read") ||
            !hasModuleAccess(a.modules, "collaboration"))
            return [];
        const data = await collaborationWorkspace(a);
        return data.cards.map((c) => {
            const terminal = c.status !== "open", own = c.owner_id === a.userId && Number(c.owner_eligible) > 0, manage = hasPermission(a, "records.review");
            return {
                id: String(c.id),
                type: "collaboration_action" as const,
                title: String(c.title),
                detail: `${c.source_type} · ${c.source_id}`,
                href: "/collaboration",
                status: String(c.status),
                updatedAt: Number(c.updated_at),
                dueAt: Number(c.due_at),
                queue: terminal
                    ? ("completed" as const)
                    : own || manage
                        ? ("todo" as const)
                        : ("waiting" as const),
                canAct: !terminal && (own || manage),
                responsibleQueue: "Assigned action owner",
                nextAction: own
                    ? "Complete with evidence"
                    : "Review routing and source context",
                blocker: Number(c.owner_eligible)
                    ? null
                    : "Assigned owner is no longer eligible; reviewer reassignment is required",
                relationships: [
                    own
                        ? ("assigned" as const)
                        : c.created_by === a.userId
                            ? ("requested" as const)
                            : ("team" as const),
                ],
            };
        });
    }
    return { collaborationWorkspace, collaborationExport, collaborationActionDetail, createRecordRelation, removeRecordRelation, createActionCard, changeActionCard, collaborationActionWork };
}
