import { z } from 'zod';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { WorkItem } from "@erpx/feature-work_projection/lib/work-item";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { hasModuleAccess } from '@erpx/catalogue/module-access';
import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import { sourceType, csvCell, type Row, type Source, type Card } from '@erpx/server-collaboration/domain/collaboration-work';
export interface CollaborationWorkRepository {
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
}
export interface OwnerCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[], scope?: Pick<WorkContext, 'companyId' | 'branchId' | 'departmentId'>): Promise<Result>;
}
export interface CollaborationWorkPorts<Effect> {
    repository: CollaborationWorkRepository;
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
            sourceTypes: sourceType.options.filter(kind=>kind!=='field_service_order'),
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
    return { collaborationWorkspace, collaborationExport, collaborationActionDetail, collaborationActionWork };
}
