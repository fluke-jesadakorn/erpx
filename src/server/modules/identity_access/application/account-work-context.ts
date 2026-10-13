import { safeReturnPath } from '../domain/access-problem';
import { parseHoldingWorkspace, type HoldingWorkspaceScope } from "@/lib/work_projection/workspace";
import type { AccountWorkContext } from "@/lib/identity_access/account-work-context";
import type { CurrentWorkspaceChoice, WorkspaceChoice } from "@/lib/work_projection/workspace-choice";
import type { ActiveWorkSelection } from '@/server/modules/work_projection/public/work-role-store';
import type { AccountAccessContext, WorkAssignment, ResolvedWorkspaceScope, PlatformContext } from '../domain/authorization';
import { assignmentApplies } from '../domain/workspace-access';
import type { IdentityError } from './authorization';
export interface AccountContextPorts {
    session(): Promise<{
        session: {
            user: {
                id: string;
                name: string;
                email: string;
            };
        };
    }>;
    choices(): Promise<WorkspaceChoice[]>;
    manage(): Promise<boolean>;
    platform(): Promise<PlatformContext>;
    access(scope: HoldingWorkspaceScope, options: {
        companyId?: string;
    }): Promise<AccountAccessContext>;
    selection(access: AccountAccessContext, input: {
        sessionId: string;
        userId: string;
        companyId: string;
        expiresAt: number;
    }, assignments: WorkAssignment[], eligible: WorkAssignment[]): Promise<ActiveWorkSelection | null>;
    assignmentChoices(holdingId: string, assignmentId: string): Promise<WorkspaceChoice[]>;
    now(): number;
    error: IdentityError;
    apiError(error: unknown): error is Error & {
        status: number;
        code: string;
    };
}
function currentChoice(scope: ResolvedWorkspaceScope): CurrentWorkspaceChoice {
    if (scope.kind === "platform")
        return scope;
    const common = { holdingSlug: scope.holdingSlug, companyId: scope.companyId };
    if (scope.kind === "holding")
        return { kind: scope.kind, ...common };
    if (scope.kind === "branch")
        return { kind: scope.kind, ...common, branchId: scope.branchId };
    return { kind: scope.kind, ...common, branchId: scope.branchId, departmentId: scope.departmentId };
}
export function createAccountWorkContext(ports: AccountContextPorts) {
    async function manageAvailable() { try {
        return await ports.manage();
    }
    catch (error) {
        if (ports.apiError(error) && [401, 403].includes(error.status))
            return false;
        if (error instanceof Error && error.message === 'FORBIDDEN')
            return false;
        throw error;
    } }
    return async (input: {
        from: string;
        companyId?: string;
    }): Promise<AccountWorkContext> => {
        const from = safeReturnPath(input.from);
        if (!from)
            throw ports.error(422, "INVALID_WORKSPACE", "Use the current workspace page.");
        const [{ session }, choices, canManageErpx] = await Promise.all([
            ports.session(), ports.choices(), manageAvailable(),
        ]);
        const result: AccountWorkContext = {
            user: { id: session.user.id, name: session.user.name, email: session.user.email },
            from, status: "forbidden", sameWorkspace: false, canUseWorkspace: false,
            requiresWorkspaceSelection: choices.length > 0 || canManageErpx,
            choices, canManageErpx, assignments: [], assignmentId: null, revision: 0, context: null,
        };
        let parts: string[];
        try {
            parts = new URL(from, "https://erpx.invalid").pathname.split("/").filter(Boolean).map(decodeURIComponent);
        }
        catch {
            throw ports.error(422, "INVALID_WORKSPACE", "Use the current workspace page.");
        }
        if (parts[0] === "platform") {
            result.current = { kind: "platform" };
            try {
                const actor = await ports.platform();
                return { ...result, status: "ready", sameWorkspace: true, canUseWorkspace: true,
                    requiresWorkspaceSelection: false, assignments: actor.assignments, assignmentId: actor.assignmentId,
                    context: { userId: actor.userId, companyId: "platform", assignmentId: actor.assignmentId, revision: 0 } };
            }
            catch (error) {
                if (!(error instanceof Error) || error.message !== "FORBIDDEN")
                    throw error;
                return { ...result, problem: { code: "FORBIDDEN", message: "Your account cannot open this workspace." } };
            }
        }
        const parsed = parseHoldingWorkspace(parts[1], parts.slice(2));
        if (!parsed)
            throw ports.error(422, "INVALID_WORKSPACE", "Use the current workspace page.");
        if (input.companyId)
            result.current = { kind: "holding", holdingSlug: parsed.scope.holdingSlug, companyId: input.companyId };
        try {
            // The explicit company is deliberate inspection evidence. Old revision
            // headers are irrelevant to this GET; account access still verifies the
            // session, assignments and company without asserting an old form's role.
            const access = await ports.access(parsed.scope, { companyId: input.companyId });
            result.current = currentChoice(access.scope);
            result.assignments = access.assignments;
            const eligible = access.assignments.filter(assignment => assignmentApplies(assignment, access.scope));
            const selection = await ports.selection(access, {
                sessionId: access.sessionId, userId: access.userId, companyId: access.scope.companyId, expiresAt: access.expiresAt,
            }, access.assignments, eligible);
            result.assignmentId = selection?.assignment_id ?? null;
            result.revision = selection?.revision ?? 0;
            const selected = selection && selection.expires_at > ports.now()
                ? access.assignments.find(assignment => assignment.id === selection.assignment_id && assignment.version === selection.assignment_version)
                : undefined;
            if (selected && eligible.some(assignment => assignment.id === selected.id)) {
                return { ...result, status: "ready", sameWorkspace: true, canUseWorkspace: true, requiresWorkspaceSelection: false,
                    context: { userId: access.userId, companyId: access.scope.companyId, assignmentId: selected.id, revision: result.revision } };
            }
            if (!selected && eligible.length) {
                return { ...result, status: "role-required", requiresWorkspaceSelection: false,
                    problem: { code: "WORK_ROLE_REQUIRED", message: "Choose an available role for this workspace." } };
            }
            if (selected) {
                const scoped = await ports.assignmentChoices(access.scope.holdingId, selected.id);
                const companyChoices = scoped.flatMap(choice => choice.kind === "holding" ? choice.companies : []);
                result.choices = choices.map(choice => choice.kind === "holding" && choice.holdingSlug === access.scope.holdingSlug
                    ? { ...choice, companies: choice.companies.flatMap(company => company.id === access.scope.companyId
                            ? companyChoices.filter(item => item.id === company.id) : [company]) }
                    : choice).filter(choice => choice.kind === "platform" || choice.companies.length > 0);
                result.requiresWorkspaceSelection = result.choices.length > 0 || canManageErpx;
            }
            return { ...result, status: result.requiresWorkspaceSelection ? "workspace-required" : "forbidden",
                problem: { code: "WORK_ROLE_UNAVAILABLE", message: "Your selected role cannot open this workspace. Choose an available workspace or role." } };
        }
        catch (error) {
            if (ports.apiError(error) && error.status === 401)
                throw error;
            const code = ports.apiError(error) ? error.code : error instanceof Error ? error.message : "";
            if (!["FORBIDDEN", "COMPANY_REQUIRED", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "INVALID_ORGANIZATION_LINEAGE"].includes(code))
                throw error;
            return { ...result, status: result.requiresWorkspaceSelection ? "workspace-required" : "forbidden",
                problem: { code, message: "Your account cannot open the current workspace. Choose an available workspace or account." } };
        }
    };
}
