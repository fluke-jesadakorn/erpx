import { z } from 'zod';
import { directoryChangeInput, type DirectoryAction, type DirectoryInventory, type DirectoryChangeInput, type DirectoryAutomationData, type DirectoryPreview } from "@erpx/feature-identity_access/lib/directory-operations";
import { attributeAccess, type AttributeDefinition } from "@erpx/feature-identity_access/lib/user-management";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { IamActor } from '@erpx/server-identity_access/domain/iam-actor';
import type { ConnectionRow } from '@erpx/server-identity_access/domain/directories';
import { connectorMappingInput, affectedObjects, type State, type ConnectorMapping, type AutomationConfiguration, type AutomationConnectionFact, type AutomationCaseFact, type IdentityLinkFact, type LifecycleFact, type PositionFact, type PreviewPreparation, type OperationStep, type DirectoryOperationJob, type ProvisionPlan, type ProvisionOperation, type ProvisionEvidence, type MembershipSource, type DueOperation, type GroupMembershipAction, type LifecycleAction } from '@erpx/server-identity_access/domain/directory-operations';
import type { IdentityError } from '@erpx/server-identity_access/application/authorization';
export interface OperationTransaction {
    connection(facts: {
        id: string;
        actor: Pick<IamActor, "companyId">;
    }): Promise<ConnectionRow | null>;
    recordInventory(facts: {
        id: string;
        actor: IamActor;
        version: number;
        inventory: DirectoryInventory;
        observedNow: number;
    }): Promise<void>;
    lockState(facts: {
        row: ConnectionRow;
    }): Promise<State | null>;
    ownedAttribute(facts: {
        actor: IamActor;
        m: ConnectorMapping;
    }): Promise<{
        definition: AttributeDefinition;
    } | null>;
    configureState(facts: {
        input: AutomationConfiguration;
        mappings: ConnectorMapping[];
        row: ConnectionRow;
    }): Promise<void>;
    state(facts: {
        row: ConnectionRow;
    }): Promise<State | null>;
    identityLinks(facts: {
        row: ConnectionRow;
        ids: string[];
    }): Promise<IdentityLinkFact[]>;
    lifecycle(facts: {
        action: LifecycleAction;
        actor: Pick<IamActor, "companyId" | "userId">;
        observedNow: number;
    }): Promise<LifecycleFact | null>;
    verifiedLifecycleAccount(facts: {
        row: ConnectionRow;
        action: LifecycleAction;
        actor: Pick<IamActor, "companyId" | "userId">;
        lifecycle: LifecycleFact;
    }): Promise<number | null>;
    relatedConnections(facts: {
        row: ConnectionRow;
    }): Promise<{
        id: string;
        company_id: string;
    }[]>;
    affectedPositions(facts: {
        relatedConnections: {
            id: string;
            company_id: string;
        }[];
        ids: string[];
    }): Promise<PositionFact[]>;
    companyNames(facts: {
        companyIds: string[];
    }): Promise<{
        id: string;
        name: string;
    }[]>;
    protectedAccount(facts: {
        i: IdentityLinkFact;
        observedNow: number;
        actor: Pick<IamActor, "companyId" | "userId">;
    }): Promise<{
        protected: boolean;
        self: boolean;
    }>;
    affectedPeople(facts: {
        identities: IdentityLinkFact[];
        positionRows: PositionFact[];
    }): Promise<{
        legal_name: string;
    }[]>;
    previewMembershipClaims(facts: {
        row: ConnectionRow;
        action: GroupMembershipAction;
    }): Promise<{
        member_id: string;
        source_id: string;
    }[]>;
    directCompanyAuthority(facts: {
        companyId: string;
        actor: IamActor;
        observedNow: number;
    }): Promise<number | null>;
    insertOperation(facts: {
        id: string;
        actor: IamActor;
        p: PreviewPreparation;
        input: DirectoryChangeInput;
        review: {
            id: string;
        } | null;
        observedNow: number;
        direct: boolean;
    }): Promise<void>;
    lockPendingOperation(facts: {
        p: Record<string, unknown>;
        actor: IamActor;
    }): Promise<DirectoryOperationJob | null>;
    beneficiaryIsReviewer(facts: {
        userId: string | null;
        actor: IamActor;
        companyId: string;
    }): Promise<{
        same: boolean;
    }>;
    reviewCompanyAuthority(facts: {
        companyId: string;
        actor: IamActor;
        observedNow: number;
    }): Promise<number | null>;
    decideOperation(facts: {
        decision: "approve" | "reject";
        observedNow: number;
        job: DirectoryOperationJob;
    }): Promise<void>;
    resumeOperation(facts: {
        id: string;
        actor: IamActor;
        version: number;
    }): Promise<{
        provision_id: string | null;
    } | null>;
    resumeProvisioning(facts: {
        row: {
            provision_id: string | null;
        };
        actor: IamActor;
    }): Promise<void>;
    cancelOperation(facts: {
        observedNow: number;
        id: string;
        actor: IamActor;
        version: number;
    }): Promise<{
        review_id: string | null;
    } | null>;
    cancelReview(facts: {
        job: {
            review_id: string | null;
        };
    }): Promise<void>;
    executableOperation(facts: {
        row: DueOperation;
    }): Promise<DirectoryOperationJob | null>;
    provisioning(facts: {
        job: DirectoryOperationJob;
    }): Promise<ProvisionOperation | null>;
    currentPlacement(facts: {
        plan: ProvisionPlan;
        job: DirectoryOperationJob;
        observedNow: number;
        op: ProvisionOperation;
    }): Promise<number | null>;
    stateForExecution(facts: {
        conn: ConnectionRow;
    }): Promise<State | null>;
    executionAuthority(facts: {
        job: DirectoryOperationJob;
        observedNow: number;
    }): Promise<number | null>;
    currentDirectAuthority(facts: {
        companyId: string;
        job: DirectoryOperationJob;
        observedNow: number;
    }): Promise<number | null>;
    approvedReview(facts: {
        job: DirectoryOperationJob;
    }): Promise<{
        reviewed_by: string;
    } | null>;
    currentReviewAuthority(facts: {
        companyId: string;
        review: {
            reviewed_by: string;
        };
        observedNow: number;
    }): Promise<number | null>;
    markRunning(facts: {
        observedNow: number;
        job: DirectoryOperationJob;
    }): Promise<void>;
    membershipSources(facts: {
        conn: ConnectionRow;
        action: GroupMembershipAction;
    }): Promise<MembershipSource[]>;
    provisioningSources(facts: {
        job: DirectoryOperationJob;
    }): Promise<{
        evidence: ProvisionEvidence;
    }>;
    completeOperation(facts: {
        result: Record<string, unknown>;
        observedNow: number;
        job: DirectoryOperationJob;
    }): Promise<void>;
    completionProvisioning(facts: {
        job: DirectoryOperationJob;
    }): Promise<{
        evidence: ProvisionEvidence;
    }>;
    recordPackageSource(facts: {
        conn: ConnectionRow;
        action: GroupMembershipAction;
        member: string;
        applicationId: string;
        observedNow: number;
    }): Promise<void>;
    recordDirectSource(facts: {
        conn: ConnectionRow;
        action: GroupMembershipAction;
        member: string;
        job: DirectoryOperationJob;
        observedNow: number;
    }): Promise<void>;
    recordVerificationAudit(facts: {
        generatedId: string;
        job: DirectoryOperationJob;
        result: Record<string, unknown>;
        observedNow: number;
    }): Promise<void>;
    recordVerificationEvent(facts: {
        job: DirectoryOperationJob;
        result: Record<string, unknown>;
        observedNow: number;
    }): Promise<void>;
    failOperation(facts: {
        retry: boolean;
        code: string;
        observedNow: number;
        row: DueOperation;
    }): Promise<void>;
    recordCompletedStep(facts: {
        job: DirectoryOperationJob;
        conn: ConnectionRow;
        action: DirectoryAction;
        steps: OperationStep[];
        replacedSourceIds: string[];
        withdrawsSources: boolean;
    }): Promise<void>;
    finalize<T>(run: () => Promise<T>): Promise<T>;
}
export interface DirectoryOperationPorts {
    error: IdentityError;
    now(): number;
    id(): string;
    hash(value: unknown): string;
    failureCode(error: unknown): string;
    companyScope(actor: IamActor): void;
    authorize(actor: IamActor, permission: string): void;
    fresh(actor: Pick<IamActor, 'userId' | 'sessionId'>): Promise<void>;
    inventory(row: ConnectionRow): Promise<DirectoryInventory>;
    validateTarget(action: DirectoryAction, inventory: DirectoryInventory): void;
    apply(row: ConnectionRow, key: string, action: DirectoryAction, before: DirectoryInventory): Promise<Record<string, unknown>>;
    command<T>(actor: IamActor, key: string, operation: string, payload: unknown, permission: string, run: (tx: OperationTransaction) => Promise<T>): Promise<T>;
    snapshot<T>(run: (tx: OperationTransaction) => Promise<T>): Promise<T>;
    review(tx: OperationTransaction, actor: IamActor, kind: string, payload: unknown, reason: string): Promise<{
        id: string;
    }>;
    withConnectionLock(row: DueOperation, run: (tx: OperationTransaction) => Promise<void>, failed: (tx: OperationTransaction, error: unknown) => Promise<void>): Promise<void>;
    automationConnections(facts: {
        actor: IamActor;
    }): Promise<AutomationConnectionFact[]>;
    automationJobs(facts: {
        actor: IamActor;
    }): Promise<DirectoryAutomationData["jobs"]>;
    lifecycleCases(facts: {
        actor: IamActor;
    }): Promise<AutomationCaseFact[]>;
    dueOperations(facts: {
        observedNow: number;
        limit: number;
    }): Promise<DueOperation[]>;
}
export function createDirectoryOperations(ports: DirectoryOperationPorts) {
    function fail(code: string, message: string, status = 409): never { throw ports.error(status, code, message); }
    function scope(actor: IamActor, permission = "iam.directory.manage") {
        if (actor.platform)
            fail("COMPANY_SCOPE_REQUIRED", "Use People in a Company workspace.", 403);
        ports.companyScope(actor);
        ports.authorize(actor, permission);
    }
    async function connection(tx: OperationTransaction, actor: Pick<IamActor, "companyId">, id: string, readOnly = false) {
        const row = (await tx.connection({ id, actor })) as ConnectionRow | undefined;
        if (!row ||
            !(readOnly ? ["draft", "tested", "active"] : ["active"]).includes(row.status))
            fail("CONNECTION_UNAVAILABLE", "Choose an available directory connection.");
        return row;
    }
    async function directoryAutomationData(actor: IamActor): Promise<DirectoryAutomationData> {
        scope(actor, "iam.read");
        const rows = (await ports.automationConnections({ actor }));
        const jobs = (await ports.automationJobs({ actor }));
        const cases = (await ports.lifecycleCases({ actor }));
        return {
            cases: cases.map((c) => ({
                id: c.id,
                name: c.name,
                kind: c.kind,
                effectiveAt: c.effective_at,
                accountIds: c.account_ids,
            })),
            connections: rows.map((r) => ({
                id: r.id,
                name: r.name,
                provider: r.provider,
                version: r.version,
                writeEnabled: r.write_enabled === true,
                testedVersion: r.automation_tested ?? null,
                inventory: r.inventory ?? null,
                mappings: r.definition?.mappings ?? [],
            })),
            jobs,
            canApply: hasPermission(actor, "iam.directory.apply"),
            canManage: hasPermission(actor, "iam.directory.manage"),
            canReview: hasPermission(actor, "iam.review"),
        };
    }
    async function refreshDirectoryInventory(actor: IamActor, id: string, version: number, key: string) {
        scope(actor);
        const row = await ports.command(actor, key + ":authorize", "directory.inventory.authorize", { id, version }, "iam.directory.manage", async (tx) => {
            const row = await connection(tx, actor, id, true);
            if (row.version !== version)
                fail("CONNECTION_CHANGED", "Refresh the connection first.");
            return row;
        });
        const inventory = await ports.inventory(row);
        return ports.command(actor, key, "directory.inventory.read", { id, version }, "iam.directory.manage", async (tx) => {
            const current = await connection(tx, actor, id, true);
            if (current.version !== version)
                fail("CONNECTION_CHANGED", "The connection changed while reading the inventory.");
            (await tx.recordInventory({ id, actor, version, inventory, observedNow: ports.now() }));
            return {
                count: inventory.objects.length,
                providerVersion: inventory.providerVersion,
            };
        });
    }
    async function configureDirectoryAutomation(actor: IamActor, input: {
        connectionId: string;
        expectedVersion: number;
        writeEnabled: boolean;
        mappings?: z.infer<typeof connectorMappingInput>[];
    }, key: string) {
        scope(actor);
        await ports.fresh(actor);
        return ports.command(actor, key, "directory.automation.configure", input, "iam.directory.manage", async (tx) => {
            const row = await connection(tx, actor, input.connectionId), state = (await tx.lockState({ row })) as State | undefined;
            if (row.version !== input.expectedVersion ||
                !state?.inventory ||
                state.tested_version !== row.version)
                fail("DIRECTORY_TEST_REQUIRED", "Test this connection version and its schema before configuring write-back.");
            const mappings = (input.mappings ??
                (state.definition.mappings as z.infer<typeof connectorMappingInput>[]) ??
                []).map((m) => connectorMappingInput.parse(m));
            if (new Set(mappings.map((m) => m.attributeId)).size !== mappings.length ||
                new Set(mappings.map((m) => m.field)).size !== mappings.length)
                fail("MAPPING_DUPLICATE", "Map each local and external field once.", 422);
            for (const m of mappings) {
                const a = (await tx.ownedAttribute({ actor, m }));
                if (!a ||
                    a.definition.target !== "person" ||
                    a.definition.archived ||
                    !["text", "checkbox"].includes(a.definition.type))
                    fail("MAPPING_ATTRIBUTE_INVALID", "Choose an active compatible person field.", 422);
                const access = attributeAccess(a.definition, actor.role, actor.scopeKind);
                if (!access.canView ||
                    (m.direction === "read" &&
                        (!access.canEdit || !hasPermission(actor, "people.manage"))))
                    fail("MAPPING_FIELD_FORBIDDEN", "The mapping requires permission to view or update the owned field.", 403);
                const f = state.inventory.fields.find((f) => f.name === m.field &&
                    f[m.direction] &&
                    f.type === a.definition.type);
                if (!f)
                    fail("MAPPING_SCHEMA_UNSUPPORTED", "This connector schema does not support the field and direction.", 422);
            }
            (await tx.configureState({ input, mappings, row }));
            return { writeEnabled: input.writeEnabled };
        });
    }
    async function prepareDirectoryPreview(tx: OperationTransaction, actor: Pick<IamActor, "companyId" | "userId">, input: DirectoryChangeInput) {
        const row = await connection(tx, actor, input.connectionId, true), state = (await tx.state({ row })) as State | undefined;
        if (row.version !== input.expectedVersion)
            fail("CONNECTION_CHANGED", "The connection changed.");
        if (!state?.inventory || state.tested_version !== row.version)
            fail("DIRECTORY_TEST_REQUIRED", "Read and test the directory inventory first.");
        const actions = [input.action, ...(input.additionalActions ?? [])];
        const simulated = structuredClone(state.inventory);
        for (const action of actions) {
            ports.validateTarget(action, simulated);
            if (row.provider === "univention" &&
                action.kind === "object.move" &&
                simulated.objects.find((o) => o.id === action.objectId)?.kind === "ou" &&
                simulated.objects.some((o) => o.parentId === action.objectId))
                fail("DIRECTORY_CONTAINER_NOT_EMPTY", "UCS recreates populated containers when moving them. Move the children first, then move the empty container and return the children to preserve permanent identities.");
            if (action.kind === "object.move")
                simulated.objects.find((o) => o.id === action.objectId)!.parentId =
                    action.parentId;
            if (action.kind === "group.members") {
                const group = simulated.objects.find((o) => o.id === action.objectId)!;
                group.memberIds =
                    action.action === "add"
                        ? [...new Set([...group.memberIds, ...action.memberIds])]
                        : group.memberIds.filter((id) => !action.memberIds.includes(id));
            }
        }
        // Each target may be mutated once per batch. Dependent edits require a new preview.
        if (actions.length > 1 &&
            actions.some((a) => a.kind === "account.enable" || a.kind === "account.disable"))
            fail("DIRECTORY_BATCH_DEPENDENCY", "Apply account activation or suspension separately to review its full impact.");
        const targets = actions.flatMap((a) => "objectId" in a && a.objectId ? [a.objectId] : []);
        if (new Set(targets).size !== targets.length)
            fail("DIRECTORY_BATCH_DEPENDENCY", "Apply one change per object, then refresh before dependent changes.");
        const objects = [
            ...new Map(actions
                .flatMap((a) => affectedObjects(a, state.inventory!))
                .map((o) => [o.id, o])).values(),
        ], ids = objects.flatMap((o) => [
            o.id,
            ...(typeof (o.properties.windowsIdentity as {
                id?: unknown;
            } | undefined)
                ?.id === "string"
                ? [(o.properties.windowsIdentity as {
                        id: string;
                    }).id]
                : []),
        ]);
        const identities = (await tx.identityLinks({ row, ids }));
        if (input.action.kind === "account.enable" ||
            (input.action.kind === "account.disable" && input.action.caseId)) {
            const action = input.action;
            const lifecycle = (await tx.lifecycle({ action, actor, observedNow: ports.now() }));
            const linked = lifecycle &&
                identities.some((i) => i.external_id === action.objectId &&
                    i.user_id &&
                    i.company_id === actor.companyId &&
                    i.person_id === lifecycle.person_id);
            const verified = lifecycle &&
                (await tx.verifiedLifecycleAccount({ row, action, actor, lifecycle }));
            if (!lifecycle || (!linked && !verified) || !verified)
                fail("VERIFIED_LIFECYCLE_REQUIRED", "Choose the lifecycle case for this explicitly verified account.");
            if (action.kind === "account.enable" &&
                (!lifecycle.active || lifecycle.status !== "completed"))
                fail("LIFECYCLE_NOT_EFFECTIVE", "Activation requires completed personal access review and the effective start time.");
            if (action.kind === "account.disable" &&
                (!["offboarding", "suspend"].includes(lifecycle.kind) ||
                    !["scheduled", "completed"].includes(lifecycle.status) ||
                    lifecycle.effective_at > ports.now()))
                fail("LIFECYCLE_NOT_EFFECTIVE", "The reviewed departure cutoff has not taken effect.");
        }
        const relatedConnections = (await tx.relatedConnections({ row }));
        const positionRows = (await tx.affectedPositions({ relatedConnections, ids }));
        const positions = positionRows.map((r) => String(r.title));
        const companyIds = [
            ...new Set([
                actor.companyId,
                ...identities.map((i) => String(i.company_id)),
                ...positionRows.map((r) => String(r.company_id)),
            ]),
        ].sort();
        const companyNames = (await tx.companyNames({ companyIds }));
        const companies = companyIds.map((id) => ({
            id,
            name: String(companyNames.find((r) => r.id === id)?.name ?? id),
        }));
        if (input.action.kind === "account.disable") {
            const targetId = input.action.objectId, target = state.inventory.objects.find((o) => o.id === targetId), aliases = [
                targetId,
                (target?.properties.windowsIdentity as {
                    id?: string;
                } | undefined)?.id,
            ], accounts = identities.filter((i) => aliases.includes(i.external_id) && i.user_id);
            for (const i of accounts) {
                const protectedAccount = (await tx.protectedAccount({ i, observedNow: ports.now(), actor }));
                if (protectedAccount.protected || protectedAccount.self)
                    fail("PROTECTED_DIRECTORY_ACCOUNT", "This operation would disable your own account or a recovery administrator.", 403);
            }
        }
        const people = (await tx.affectedPeople({ identities, positionRows })).map((r) => String(r.legal_name));
        const resultSources: string[] = [];
        for (const action of actions)
            if (action.kind === "group.members" && action.action === "remove") {
                const claims = (await tx.previewMembershipClaims({ row, action }));
                // Preview exposes sources. The provisioning service removes only its replaced claims.
                if (claims.length)
                    resultSources.push(...claims.map((v) => `Membership ${v.member_id} is supplied by applied package ${v.source_id}.`));
            }
        const evidence = {
            connectionVersion: row.version,
            stateVersion: state.version,
            inventory: state.inventory,
            objects: ids,
            companies: companyIds,
            identityLinks: identities.map((i): [
                string,
                string | null,
                string
            ] => [i.id, i.user_id, i.company_id]),
        };
        const result: DirectoryPreview = {
            token: ports.hash({ input, evidence }),
            objects: objects.map((o) => o.name + " · " + o.kind),
            companies,
            people,
            computers: objects.filter((o) => o.kind === "computer").map((o) => o.name),
            positions,
            warnings: resultSources,
        };
        if (!state.write_enabled)
            result.warnings.push("Read-only connection: IT can preview work; enable tested write-back before submission.");
        if (input.action.kind === "object.move")
            result.warnings.push("Moving an OU placement changes inherited Windows policies.");
        if (input.action.kind === "account.disable" && companyIds.length > 1)
            result.warnings.push("Disabling this directory account affects every listed Company.");
        if (input.action.kind === "gpo.publish")
            result.warnings.push("Published GPO configuration is separate from confirmed results on endpoints.");
        return { result, evidence, state, row };
    }
    async function submitDirectoryChange(actor: IamActor, raw: DirectoryChangeInput, token: string, key: string, direct = false) {
        scope(actor, direct ? "iam.directory.apply" : "iam.directory.manage");
        await ports.fresh(actor);
        const input = directoryChangeInput.parse(raw);
        return ports.command(actor, key, direct ? "directory.operation.apply" : "directory.operation.propose", input, direct ? "iam.directory.apply" : "iam.directory.manage", async (tx) => {
            const p = await prepareDirectoryPreview(tx, actor, input);
            if (p.result.token !== token)
                fail("DIRECTORY_PREVIEW_CHANGED", "Review the latest directory impact before submitting.");
            if (!p.state.write_enabled)
                fail("DIRECTORY_READ_ONLY", "Enable tested write-back or keep this as a manual IT task.");
            if (p.row.status !== "active")
                fail("DIRECTORY_READ_ONLY", "Activate the tested connection through independent review first.");
            if (input.action.kind === "account.disable" &&
                p.result.companies.some((c) => !input.affectedCompanyIds.includes(c.id)))
                fail("DIRECTORY_IMPACT_ACK_REQUIRED", "Acknowledge every affected Company before disabling the whole account.");
            if (direct)
                for (const companyId of p.evidence.companies) {
                    if (!(await tx.directCompanyAuthority({ companyId, actor, observedNow: ports.now() })))
                        fail("DIRECTORY_APPLY_SCOPE", "You must administer every affected Company.", 403);
                }
            const id = ports.id(), review = direct
                ? null
                : await ports.review(tx, actor, "directory.operation", { operationId: id }, input.reason);
            (await tx.insertOperation({ id, actor, p, input, review, observedNow: ports.now(), direct }));
            return {
                id,
                reviewId: review?.id ?? null,
                status: direct ? "queued" : "pending_review",
            };
        });
    }
    async function applyDirectoryOperationReview(tx: OperationTransaction, actor: IamActor, p: Record<string, unknown>, decision: "approve" | "reject" = "approve") {
        const job = (await tx.lockPendingOperation({ p, actor }));
        if (!job)
            fail("DIRECTORY_OPERATION_CHANGED", "This directory operation changed.");
        if (decision === "approve") {
            ports.authorize(actor, "iam.directory.manage");
            const current = await prepareDirectoryPreview(tx, actor, job.definition);
            if (ports.hash(current.evidence) !== ports.hash(job.evidence) ||
                !current.state.write_enabled)
                fail("DIRECTORY_PREVIEW_CHANGED", "The directory preview changed. Cancel and submit a fresh proposal.");
            if (["account.enable", "group.members"].includes(job.definition.action.kind))
                for (const [, userId, companyId] of current.evidence.identityLinks)
                    if (userId &&
                        (await tx.beneficiaryIsReviewer({ userId, actor, companyId })).same)
                        fail("DIRECTORY_BENEFICIARY_REVIEW", "A person affected by this access change cannot approve it.", 403);
            for (const companyId of current.evidence.companies)
                if (companyId !== actor.companyId &&
                    !(await tx.reviewCompanyAuthority({ companyId, actor, observedNow: ports.now() })))
                    fail("DIRECTORY_REVIEW_SCOPE", "The reviewer must cover every affected Company.", 403);
        }
        (await tx.decideOperation({ decision, observedNow: ports.now(), job }));
    }
    async function retryDirectoryOperation(actor: IamActor, id: string, version: number, key: string) {
        scope(actor, "iam.directory.apply");
        await ports.fresh(actor);
        return ports.command(actor, key, "directory.operation.retry", { id, version }, "iam.directory.apply", async (tx) => {
            const row = (await tx.resumeOperation({ id, actor, version }));
            if (!row)
                fail("DIRECTORY_RETRY_UNAVAILABLE", "A stale or uncertain change needs a new preview. Only the original operator can resume a transport or MFA retry.");
            if (row.provision_id)
                (await tx.resumeProvisioning({ row, actor }));
            return { id, status: "queued" };
        });
    }
    async function cancelDirectoryOperation(actor: IamActor, id: string, version: number, key: string) {
        scope(actor);
        return ports.command(actor, key, "directory.operation.cancel", { id, version }, "iam.directory.manage", async (tx) => {
            const job = (await tx.cancelOperation({ observedNow: ports.now(), id, actor, version }));
            if (!job)
                fail("DIRECTORY_CANCEL_UNAVAILABLE", "Only work that has not started can be cancelled.");
            (await tx.cancelReview({ job }));
            return { id, status: "cancelled" };
        });
    }
    async function drainDirectoryOperations(limit = 10) {
        const due = await ports.dueOperations({ observedNow: ports.now(), limit });
        for (const row of due)
            await ports.withConnectionLock(row, async (tx) => {
                const job = (await tx.executableOperation({ row }));
                if (!job)
                    return;
                if (job.provision_id) {
                    const op = (await tx.provisioning({ job }));
                    if (!op || !["queued", "running", "retry"].includes(op.status))
                        fail("PROVISIONING_CHANGED", "This provisioning operation is no longer executable.");
                    for (const plan of op.evidence.plans) {
                        if (!(await tx.currentPlacement({ plan, job, observedNow: ports.now(), op })))
                            fail("PROVISIONING_CHANGED", "HR placement or employee access changed. Preview again.");
                    }
                }
                const conn = await connection(tx, { companyId: job.company_id }, job.connection_id);
                const state = (await tx.stateForExecution({ conn })) as State;
                const allowed = (await tx.executionAuthority({ job, observedNow: ports.now() }));
                if (!allowed ||
                    !state?.write_enabled ||
                    conn.version !== job.connection_version ||
                    state.version !== job.state_version)
                    fail("DIRECTORY_AUTHORITY_CHANGED", "The authorizing role or connector configuration changed.");
                const impact = await prepareDirectoryPreview(tx, { companyId: job.company_id, userId: job.created_by } as IamActor, job.definition);
                if (job.approval_mode === "direct") {
                    await ports.fresh({
                        userId: job.created_by,
                        sessionId: job.mfa_session_id,
                    });
                    for (const companyId of impact.evidence.companies)
                        if (!(await tx.currentDirectAuthority({ companyId, job, observedNow: ports.now() })))
                            fail("DIRECTORY_APPLY_SCOPE", "The operator's Company authority changed.", 403);
                }
                else {
                    const review = (await tx.approvedReview({ job }));
                    for (const companyId of impact.evidence.companies)
                        if (!review ||
                            !(await tx.currentReviewAuthority({ companyId, review, observedNow: ports.now() })))
                            fail("DIRECTORY_REVIEW_AUTHORITY_CHANGED", "The directory reviewer's authority changed.");
                }
                if (ports.hash(impact.evidence.identityLinks) !==
                    ports.hash(job.evidence.identityLinks) ||
                    ports.hash(impact.evidence.companies) !== ports.hash(job.evidence.companies))
                    fail("DIRECTORY_IDENTITY_CHANGED", "Account links or affected Companies changed after review.");
                (await tx.markRunning({ observedNow: ports.now(), job }));
                const actions: DirectoryAction[] = [
                    job.definition.action,
                    ...(job.definition.additionalActions ?? []),
                ];
                const steps: {
                    index: number;
                    status: string;
                    result: Record<string, unknown>;
                }[] = job.steps ?? [];
                for (const [index, action] of actions.entries()) {
                    if (steps.some((s) => s.index === index && s.status === "completed"))
                        continue;
                    let executable = action;
                    let retainedMemberIds: string[] = [];
                    let replacedSourceIds: string[] = [];
                    if (action.kind === "group.members" && action.action === "remove") {
                        // Source ownership can change after a reviewed preview. The connection
                        // session lock is also held by the GPO reconciler through its provider
                        // write and source commit, so this is the last safe ownership check.
                        const sources = (await tx.membershipSources({ conn, action }));
                        if (job.provision_id) {
                            const provision = (await tx.provisioningSources({ job }));
                            replacedSourceIds = provision.evidence.plans.flatMap((plan: {
                                oldIds: string[];
                            }) => plan.oldIds);
                            const replaced = new Set(replacedSourceIds);
                            retainedMemberIds = action.memberIds.filter(member => {
                                const claims = sources.filter(source => source.member_id === member);
                                return !claims.length || claims.some(source => !source.application_id || !replaced.has(source.application_id));
                            });
                            executable = { ...action, memberIds: action.memberIds.filter(member => !retainedMemberIds.includes(member)) };
                        }
                        else if (sources.some(source => source.package_owned || source.gpo_owned)) {
                            fail("DIRECTORY_MEMBERSHIP_SOURCE_RETAINED", "Remove the supplying organization policy or access package before removing this membership.");
                        }
                    }
                    const result = executable.kind === "group.members" && executable.action === "remove" && !executable.memberIds.length
                        ? { verified: true, objectId: executable.objectId, retainedMemberIds }
                        : { ...await ports.apply(conn, (actions.length === 1 ? job.id : `${job.id}:${index}`) + (retainedMemberIds.length ? `:${ports.hash(executable)}` : ""), executable, job.evidence.inventory), ...(retainedMemberIds.length ? { retainedMemberIds } : {}) };
                    if (result.verified !== true)
                        fail("DIRECTORY_READBACK_FAILED", "The provider has not verified this operation.");
                    steps.push({ index, status: "completed", result });
                    const withdrawsSources = job.provision_id && action.kind === "group.members" && action.action === "remove";
                    await tx.recordCompletedStep({ job, conn, action, steps, replacedSourceIds, withdrawsSources: !!withdrawsSources });
                }
                const result = {
                    ...steps.at(-1)?.result,
                    verified: true,
                    endpointConfirmed: false,
                    steps,
                };
                await tx.finalize(async () => {
                    (await tx.completeOperation({ result, observedNow: ports.now(), job }));
                    if (job.provision_id) {
                        const provision = (await tx.completionProvisioning({ job }));
                        for (const action of actions) {
                            if (action.kind === "group.members" && action.action === "add") {
                                for (const plan of provision.evidence.plans)
                                    if (plan.definition.connectionId === conn.id &&
                                        plan.definition.directoryGroupIds.includes(action.objectId)) {
                                        for (const member of action.memberIds.filter((id) => plan.directoryMembers.includes(id))) {
                                            const applicationId = job.provision_id + ":" + plan.id;
                                            (await tx.recordPackageSource({ conn, action, member, applicationId, observedNow: ports.now() }));
                                        }
                                    }
                            }
                        }
                    }
                    if (!job.provision_id)
                        for (const action of actions)
                            if (action.kind === "group.members" && action.action === "add") {
                                for (const member of action.memberIds)
                                    (await tx.recordDirectSource({ conn, action, member, job, observedNow: ports.now() }));
                            }
                    (await tx.recordVerificationAudit({ generatedId: ports.id(), job, result, observedNow: ports.now() }));
                    (await tx.recordVerificationEvent({ job, result, observedNow: ports.now() }));
                });
            }, async (tx, error) => {
                const code = ports.failureCode(error), retry = [
                    "DIRECTORY_UNAVAILABLE",
                    "DIRECTORY_REQUEST_FAILED",
                    "DIRECTORY_READBACK_FAILED",
                ].includes(code);
                (await tx.failOperation({ retry, code, observedNow: ports.now(), row }));
            });
    }
    async function previewDirectoryChange(actor: IamActor, input: DirectoryChangeInput) { scope(actor); return ports.snapshot(async (tx) => (await prepareDirectoryPreview(tx, actor, input)).result); }
    return { directoryAutomationData, refreshDirectoryInventory, configureDirectoryAutomation, prepareDirectoryPreview, previewDirectoryChange, submitDirectoryChange, applyDirectoryOperationReview, retryDirectoryOperation, cancelDirectoryOperation, drainDirectoryOperations };
}
