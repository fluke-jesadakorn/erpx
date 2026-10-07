import { z } from 'zod';
import { inspectorInput, recordPermissions } from '../domain/inspector';
import type { WorkContext } from '../domain/authorization';
import type { IamActor } from '../domain/iam-actor';
import { samePerson, type AccessDecision } from '@/lib/iam';
import type { FlowKind } from '@/lib/flow';
import type { createIamServiceReads } from './iam-service-reads';
import type { IdentityError } from './authorization';
export function createIamInspector(ports: {
    error: IdentityError;
    apiError(error: unknown): boolean;
    overview(actor: IamActor): ReturnType<ReturnType<typeof createIamServiceReads>['iamOverview']>;
    person(companyId: string, userId: string): Promise<string | null | undefined>;
    aliases(companyId: string, personId: string): Promise<{
        user_id: string;
    }[]>;
    access(actor: WorkContext, kind: FlowKind, id: string): Promise<unknown>;
    record(companyId: string, kind: FlowKind, id: string): Promise<Record<string, unknown>>;
    flow(actor: WorkContext, kind: FlowKind, id: string): Promise<{
        canAct: boolean;
        blocker: string | null;
    }>;
}) {
    return async (admin: IamActor, input: z.infer<typeof inspectorInput>) => {
        const data = await ports.overview(admin);
        const assignment = data.assignments.find((a) => a.id === input.assignmentId && a.user_id === input.userId);
        if (!assignment)
            throw ports.error(404, "ASSIGNMENT_NOT_FOUND", "Choose an assignment within your administration scope.");
        const source = data.decisions
            .find((d) => d.assignmentId === input.assignmentId)
            ?.decisions.find((d) => d.permission === input.permission);
        if (!source)
            throw ports.error(422, "PERMISSION_UNAVAILABLE", "Choose a catalog action.");
        const decision: AccessDecision = {
            ...source,
            reasons: [...source.reasons],
            conditions: [...source.conditions],
        };
        const result = () => ({
            ...decision,
            executionAuthorized: false,
            recordId: input.recordId ?? null,
            recordType: input.recordType ?? null,
            workspaceId: input.workspaceId ?? assignment.scope_node_id ?? admin.companyId,
        });
        if (["denied", "unavailable"].includes(decision.result))
            return result();
        let scopeKind = String(assignment.scope_kind), branchId = assignment.branch_id as string | null, departmentId = scopeKind === "department" ? String(assignment.scope_node_id) : null;
        if (input.workspaceId && input.workspaceId !== admin.companyId) {
            const node = data.nodes.find((n) => n.id === input.workspaceId);
            if (!node ||
                (assignment.scope_kind === "department" &&
                    assignment.scope_node_id !== node.id) ||
                (assignment.scope_kind === "branch" &&
                    assignment.scope_node_id !== node.id &&
                    assignment.scope_node_id !== node.branch_id)) {
                decision.result = "denied";
                decision.reasons.push("This assignment does not cover the selected workspace.");
                return result();
            }
            scopeKind = String(node.kind);
            departmentId = scopeKind === "department" ? String(node.id) : null;
            branchId =
                scopeKind === "branch"
                    ? String(node.id)
                    : (node.branch_id as string | null);
        }
        else if (input.workspaceId === admin.companyId &&
            assignment.scope_kind !== "holding" &&
            !admin.platform) {
            decision.result = "denied";
            decision.reasons.push("A branch or department assignment cannot act at company scope.");
            return result();
        }
        if ((departmentId && input.permission.startsWith("inventory.")) ||
            (branchId && input.permission === "inventory.approve")) {
            decision.result = "denied";
            decision.reasons.push("Inventory policy requires branch or company scope; adjustment approval requires company scope.");
            return result();
        }
        if (!input.recordId) {
            if (input.recordType)
                decision.conditions.push("Choose a record to evaluate participation and workflow state.");
            return result();
        }
        if (!input.recordType || admin.platform) {
            decision.result = "conditional";
            decision.conditions.push("Select a supported customer record type to inspect its policy.");
            return result();
        }
        if (!recordPermissions[input.recordType].includes(input.permission)) {
            decision.result = "unavailable";
            decision.reasons.push("This action does not apply to the selected record type.");
            return result();
        }
        const role = data.roles.find((r) => r.id === assignment.role)!;
        const person = await ports.person(admin.companyId, input.userId);
        const aliases = person
            ? (await ports.aliases(admin.companyId, person)).map((r) => r.user_id)
            : [];
        // Deliberately omit authority/session: this object can only reach read policy helpers.
        const target = {
            inspection: true,
            userId: input.userId,
            name: "",
            email: "",
            role: role.id,
            assignmentId: input.assignmentId,
            companyId: admin.companyId,
            holdingId: admin.holdingId!,
            holdingSlug: "",
            holdingName: "",
            branchId,
            departmentId,
            assignmentScope: assignment.scope_kind,
            scope: {
                kind: scopeKind,
                companyId: admin.companyId,
                branchId,
                departmentId,
                holdingId: admin.holdingId,
                holdingSlug: "",
                holdingName: "",
            },
            inherited: scopeKind !== assignment.scope_kind,
            modules: role.permissions
                .filter((p) => p.startsWith("module."))
                .map((p) => p.split(".")[1]),
            assignments: [],
            workspaces: [],
            iam: {
                permissions: role.permissions,
                grantablePermissions: [],
                roleRevision: role.revision,
                identityVersion: Number(assignment.identity_version ?? 0),
                personId: person ?? null,
                personUserIds: [input.userId, ...aliases],
                principalId: input.userId,
                freshUntil: null,
            },
        } as WorkContext;
        try {
            await ports.access(target, input.recordType, input.recordId);
            if (input.permission.startsWith("self.")) {
                const row = await ports.record(admin.companyId, input.recordType, input.recordId);
                const participant = [
                    row.requester_user_id,
                    row.claimant_user_id,
                    row.creator_user_id,
                    row.created_by,
                    ...(input.recordType === "staff_task" ? [row.assignee_user_id] : []),
                ].some((id) => typeof id === "string" && samePerson(target, id));
                if (!participant) {
                    decision.result = "denied";
                    decision.reasons.push("This person is not an owner or assigned participant of this record.");
                    return result();
                }
            }
            decision.reasons.push("The current record read policy permits this person in the selected workspace.");
            if (input.permission.startsWith("self.") ||
                /\.read$/.test(input.permission)) {
                decision.result = "allowed";
                decision.conditions = [];
            }
            else {
                const flow = await ports.flow(target, input.recordType, input.recordId);
                decision.reasons.push(flow.canAct
                    ? "The person is eligible for the current workflow step."
                    : "The person cannot execute the current workflow step.");
                if (flow.blocker)
                    decision.reasons.push(flow.blocker);
                // A permission can cover multiple commands. Readiness, amounts and command inputs
                // remain mandatory and cannot be inferred from a record identifier alone.
                decision.result = "conditional";
                decision.conditions.push("The concrete command must pass its transaction, input, amount and current-record checks.");
            }
        }
        catch (error) {
            if (!(ports.apiError(error)))
                throw error;
            decision.result = "denied";
            decision.reasons.push("The record is unavailable under the selected person, assignment and workspace policy.");
        }
        return result();
    };
}
