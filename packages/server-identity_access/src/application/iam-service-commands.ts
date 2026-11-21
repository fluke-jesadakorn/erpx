import { createIamGrantPolicy } from '@erpx/server-identity_access/application/iam-grants';
import { z } from 'zod';
import { grantInput, reviewInput, type IamAssignmentFact, type IamRoleFact, type IamReviewPayload, type IamReviewFact, type IamReviewItemFact, type IamScopeNodeFact, type IamMemberFact, type GrantInput, type ReviewInput } from '@erpx/server-identity_access/domain/iam-service';
import type { IamActor } from '@erpx/server-identity_access/domain/iam-actor';
import { iamPermission } from '@erpx/server-identity_access/domain/iam-actor';
import type { IdentityError } from '@erpx/server-identity_access/application/authorization';
export interface IamServiceTransaction {
    insertIndependentReview(facts: {
        id: string;
        kind: string;
        payload: unknown;
        observedNow: number;
        reason: string;
    }): Promise<void>;
    grantSamePerson(facts: {
        input: GrantInput;
    }): Promise<{
        same?: boolean;
        ok?: boolean;
        allowed?: boolean;
        active?: boolean;
    }>;
    lockGrantParent(facts: {
        parentId: string;
    }): Promise<IamAssignmentFact | null>;
    grantParentActive(facts: {
        parentId: string;
        observedNow: number;
    }): Promise<{
        same?: boolean;
        ok?: boolean;
        allowed?: boolean;
        active?: boolean;
    }>;
    grantReviewerExpiry(facts: Record<string, never>): Promise<{
        valid_until: number;
    } | null>;
    grantParentPermissions(facts: {
        parent: IamAssignmentFact;
    }): Promise<{
        grantable_json: string[];
    } | null>;
    grantTargetMember(facts: {
        input: GrantInput;
    }): Promise<IamMemberFact | null>;
    grantPublishedRole(facts: {
        input: GrantInput;
    }): Promise<IamRoleFact | null>;
    grantScopeNode(facts: {
        input: GrantInput;
    }): Promise<IamScopeNodeFact | null>;
    grantModulePermissions(facts: {
        input: GrantInput;
    }): Promise<{
        permissions_json: string[];
    }[]>;
    insertPlatformGrant(facts: {
        id: string;
        input: GrantInput;
        modules: (string | undefined)[];
        observedNow: number;
        parentId: string;
        source: string;
        sourceId: string | null;
    }): Promise<void>;
    insertCompanyGrant(facts: {
        id: string;
        input: GrantInput;
        modules: (string | undefined)[];
        observedNow: number;
        parentId: string;
        source: string;
        sourceId: string | null;
    }): Promise<void>;
    lockRevokedAssignment(facts: {
        id: string;
    }): Promise<IamAssignmentFact | null>;
    revokeSamePerson(facts: {
        row: IamAssignmentFact;
    }): Promise<{
        same?: boolean;
        ok?: boolean;
        allowed?: boolean;
        active?: boolean;
    }>;
    revokeRecoveryMember(facts: {
        row: IamAssignmentFact;
    }): Promise<number | null>;
    otherRecoveryAdministrators(facts: {
        row: IamAssignmentFact;
        observedNow: number;
    }): Promise<number | null>;
    otherAdministrators(facts: {
        row: IamAssignmentFact;
        observedNow: number;
    }): Promise<{
        id: string;
    }[]>;
    revokeAssignment(facts: {
        observedNow: number;
        id: string;
    }): Promise<void>;
    clearRevokedWorkSelection(facts: {
        id: string;
    }): Promise<void>;
    lockIndependentReview(facts: {
        id: string;
    }): Promise<IamReviewFact | null>;
    independentSamePerson(facts: {
        r: IamReviewFact;
    }): Promise<{
        same?: boolean;
        ok?: boolean;
        allowed?: boolean;
        active?: boolean;
    }>;
    branchCoversReview(facts: {
        p: IamReviewPayload;
    }): Promise<number | null>;
    proposerCurrentAuthority(facts: {
        p: IamReviewPayload;
        r: IamReviewFact;
        observedNow: number;
    }): Promise<IamRoleFact | null>;
    proposerHasPermission(facts: {
        original: IamRoleFact;
        required: string;
    }): Promise<{
        same?: boolean;
        ok?: boolean;
        allowed?: boolean;
        active?: boolean;
    }>;
    reviewerAffectedByRole(facts: {
        p: IamReviewPayload;
        observedNow: number;
    }): Promise<number | null>;
    lockRolePublication(facts: {
        p: IamReviewPayload;
    }): Promise<IamRoleFact | null>;
    publishRoleRevision(facts: {
        p: IamReviewPayload;
    }): Promise<void>;
    invalidateRoleAssignments(facts: {
        p: IamReviewPayload;
    }): Promise<void>;
    clearPublishedWorkSelections(facts: {
        p: IamReviewPayload;
    }): Promise<void>;
    recordIndependentDecision(facts: {
        input: ReviewInput;
        observedNow: number;
        id: string;
    }): Promise<void>;
}
export interface IamServiceCommandPorts {
    error: IdentityError;
    now(): number;
    id(): string;
    companyScope(actor: IamActor): void;
    authorize(actor: IamActor, permission: string): unknown;
    fresh(actor: IamActor): Promise<void>;
    userExists(userId: string): Promise<boolean>;
    command<T>(actor: IamActor, key: string, operation: string, payload: unknown, permission: string, run: (tx: IamServiceTransaction) => Promise<T>): Promise<T>;
}
export function createIamServiceCommands(ports: IamServiceCommandPorts) {
    function fail(code: string, message: string): never { throw ports.error(409, code, message); }
    async function review(tx: IamServiceTransaction, actor: IamActor, kind: string, payload: unknown, reason: string) {
        const id = ports.id();
        (await tx.insertIndependentReview({ id, kind, payload, observedNow: ports.now(), reason }));
        return {
            id,
            status: "pending",
            message: "Submitted for independent review.",
        };
    }
    const {validateIamGrant,insertIamGrant} = createIamGrantPolicy(ports);

    async function grantIamAccess(actor: IamActor, input: z.infer<typeof grantInput>, key: string) {
        input = { ...input };
        const target = await ports.userExists(input.userId);
        if (!target)
            throw ports.error(404, "USER_NOT_FOUND", "Choose an existing account.");
        await ports.fresh(actor);
        return ports.command(actor, key, "assignment.propose", input, "iam.manage", async (tx) => {
            await validateIamGrant(tx, actor, input);
            return review(tx, actor, "assignment.grant", {
                ...input,
                parentId: actor.assignmentId,
                grantablePermissions: actor.iam?.grantablePermissions ?? [],
            }, input.reason);
        });
    }
    async function revokeIamAssignment(actor: IamActor, id: string, version: number, reason: string, key: string) {
        return ports.command(actor, key, "assignment.revoke", { id, version, reason }, "iam.manage", async (tx) => {
            return applyIamRevocation(tx, actor, id, version, reason);
        });
    }
    async function applyIamRevocation(tx: IamServiceTransaction, actor: IamActor, id: string, version: number, reason: string, approved = false) {
        const row = (await tx.lockRevokedAssignment({ id }));
        if (!row)
            throw ports.error(404, "ASSIGNMENT_NOT_FOUND", "Assignment not found.");
        if (row.user_id === actor.userId ||
            (await tx.revokeSamePerson({ row })).same)
            throw ports.error(403, "SELF_ACCESS_CHANGE", "Another administrator must change your access.");
        if (Number(row.version) !== version)
            fail("ASSIGNMENT_CHANGED", "This assignment changed.");
        const recovery = (await tx.revokeRecoveryMember({ row }));
        if (recovery) {
            const others = (await tx.otherRecoveryAdministrators({ row, observedNow: ports.now() }));
            if (!others)
                fail("LAST_RECOVERY_ADMINISTRATOR", "Designate another verified local recovery administrator first.");
        }
        if (["holding_owner", "platform_admin"].includes(row.role)) {
            const other = (await tx.otherAdministrators({ row, observedNow: ports.now() }));
            if (!other.length)
                fail("LAST_ADMINISTRATOR", "The last recovery administrator must remain available.");
        }
        if (!actor.platform &&
            actor.scopeKind !== "holding" &&
            row.scope_node_id !== actor.scopeNodeId)
            throw ports.error(403, "SCOPE_FORBIDDEN", "This assignment is outside your scope.");
        if (recovery && !approved) {
            await ports.fresh(actor);
            ports.authorize(actor, iamPermission(actor, "iam.recovery"));
            return review(tx, actor, "emergency.revoke", { id, version, reason }, reason);
        }
        (await tx.revokeAssignment({ observedNow: ports.now(), id }));
        if (!actor.platform)
            (await tx.clearRevokedWorkSelection({ id }));
        return { id, status: "revoked" };
    }
    async function decideIamReview(actor: IamActor, id: string, input: z.infer<typeof reviewInput>, key: string, applyExtension?: (tx: IamServiceTransaction, kind: string, payload: Record<string, unknown>) => Promise<void>, rejectExtension?: (tx: IamServiceTransaction, kind: string, payload: Record<string, unknown>) => Promise<void>) {
        ports.companyScope(actor);
        await ports.fresh(actor);
        return ports.command(actor, key, "review.decide", { id, ...input }, "iam.review", async (tx) => {
            const r = (await tx.lockIndependentReview({ id }));
            if (!r)
                throw ports.error(404, "REVIEW_NOT_FOUND", "Review not found.");
            if (r.status !== "pending" || Number(r.version) !== input.expectedVersion)
                fail("REVIEW_CHANGED", "This review changed.");
            if (r.created_by === actor.userId ||
                r.created_person === (actor.iam?.personId ?? actor.userId) ||
                (await tx.independentSamePerson({ r })).same)
                throw ports.error(403, "INDEPENDENT_REVIEW_REQUIRED", "A different person must review this change.");
            const p = r.payload;
            if (!["holding", "platform"].includes(actor.scopeKind)) {
                const covered = ["assignment.grant", "mapping.activate", "policy.grant"].includes(r.kind) &&
                    p.scopeKind !== "holding" &&
                    p.scopeNodeId &&
                    (p.scopeNodeId === actor.scopeNodeId ||
                        (actor.scopeKind === "branch" &&
                            (await tx.branchCoversReview({ p }))));
                if (!covered)
                    throw ports.error(403, "REVIEW_OUTSIDE_SCOPE", "This review is outside your delegated scope.");
            }
            if (input.decision === "approve") {
                const original = (await tx.proposerCurrentAuthority({ p, r, observedNow: ports.now() }));
                if (!original)
                    fail("PROPOSER_AUTHORITY_REVOKED", "The proposer no longer has active authority. Create a new proposal.");
                const required = r.kind === "people.provision.grant"
                    ? "iam.directory.apply"
                    : r.kind === "role.publish"
                        ? "iam.roles.manage"
                        : ["connection.activate", "directory.operation"].includes(r.kind)
                            ? "iam.directory.manage"
                            : r.kind.startsWith("emergency.")
                                ? "iam.recovery"
                                : "iam.manage";
                if (!(await tx.proposerHasPermission({ original, required })).allowed)
                    fail("PROPOSER_AUTHORITY_REVOKED", "The proposer no longer has permission for this change.");
                if (r.kind === "role.publish" &&
                    [...p.permissions, ...p.grantablePermissions].some((v: string) => !original.grantable_json.includes(v)))
                    fail("DELEGATION_EXCEEDED", "The revision exceeds the original delegated permission set.");
                if (r.kind === "role.publish") {
                    if ((await tx.reviewerAffectedByRole({ p, observedNow: ports.now() })))
                        throw ports.error(403, "INDEPENDENT_REVIEW_REQUIRED", "A person affected by this role revision cannot approve it.");
                    const role = (await tx.lockRolePublication({ p }));
                    if (!role ||
                        role.built_in ||
                        Number(role.published_revision) !== p.expectedVersion)
                        fail("ROLE_CHANGED", "The published role changed. Propose a new revision.");
                    if ([...p.permissions, ...p.grantablePermissions].some((v: string) => !actor.iam?.grantablePermissions.includes(v)))
                        throw ports.error(403, "DELEGATION_EXCEEDED", "This revision exceeds your review authority.");
                    (await tx.publishRoleRevision({ p }));
                    (await tx.invalidateRoleAssignments({ p }));
                    if (!actor.platform)
                        (await tx.clearPublishedWorkSelections({ p }));
                }
                else if (r.kind === "emergency.revoke") {
                    ports.authorize(actor, iamPermission(actor, "iam.recovery"));
                    await applyIamRevocation(tx, actor, p.id, p.version, p.reason, true);
                }
                else if (r.kind === "assignment.grant") {
                    const grant = grantInput.parse(p);
                    await validateIamGrant(tx, actor, grant, p.parentId);
                    await insertIamGrant(tx, actor, grant, p.parentId);
                }
                else if (applyExtension)
                    await applyExtension(tx, r.kind, p);
                else
                    fail("REVIEW_UNSUPPORTED", "This review requires its registered workflow handler.");
            }
            if (input.decision === "reject" && rejectExtension)
                await rejectExtension(tx, r.kind, p);
            (await tx.recordIndependentDecision({ input, observedNow: ports.now(), id }));
            return {
                id,
                status: input.decision === "approve" ? "approved" : "rejected",
            };
        });
    }
    return { review, validateIamGrant, insertIamGrant, grantIamAccess, revokeIamAssignment, applyIamRevocation, decideIamReview };
}
