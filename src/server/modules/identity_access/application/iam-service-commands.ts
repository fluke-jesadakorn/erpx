import { z } from 'zod';
import { roleInput, grantInput, reviewInput, accessReviewCampaignInput, accessReviewDecisionInput, type IamAssignmentFact, type IamRoleFact, type IamReviewPayload, type IamReviewFact, type IamReviewItemFact, type IamCampaignFact, type IamScopeNodeFact, type IamMemberFact, type RoleInput, type GrantInput, type ReviewInput, type CampaignInput, type CampaignDecisionInput } from '../domain/iam-service';
import type { IamActor } from '../domain/iam-actor';
import { iamPermission } from '../domain/iam-actor';
import { permissionCatalog } from '@/lib/permissions';
import type { IdentityError } from './authorization';
export interface IamServiceTransaction {
    activeReviewAssignments(facts: {
        observedNow: number;
    }): Promise<IamAssignmentFact[]>;
    insertReviewCampaign(facts: {
        id: string;
        input: CampaignInput;
        now: number;
    }): Promise<void>;
    insertReviewItem(facts: {
        key: string;
        index: number;
        id: string;
        a: IamAssignmentFact;
    }): Promise<void>;
    insertCampaignStartedEvent(facts: {
        key: string;
        id: string;
        input: CampaignInput;
        assignments: IamAssignmentFact[];
        now: number;
    }): Promise<void>;
    lockReviewItem(facts: {
        itemId: string;
    }): Promise<IamReviewItemFact | null>;
    reviewerSamePerson(facts: {
        item: IamReviewItemFact;
    }): Promise<{
        same?: boolean;
        ok?: boolean;
        allowed?: boolean;
        active?: boolean;
    }>;
    lockReviewAssignment(facts: {
        item: IamReviewItemFact;
    }): Promise<IamAssignmentFact | null>;
    reviewAssignmentActive(facts: {
        item: IamReviewItemFact;
        observedNow: number;
    }): Promise<{
        same?: boolean;
        ok?: boolean;
        allowed?: boolean;
        active?: boolean;
    }>;
    reviewRecoveryMember(facts: {
        item: IamReviewItemFact;
    }): Promise<number | null>;
    recordItemDecision(facts: {
        input: CampaignDecisionInput;
        now: number;
        itemId: string;
    }): Promise<void>;
    insertDecisionEvent(facts: {
        key: string;
        item: IamReviewItemFact;
        itemId: string;
        input: CampaignDecisionInput;
        now: number;
    }): Promise<void>;
    pendingReviewItems(facts: {
        item: IamReviewItemFact;
    }): Promise<{
        n: number;
    }>;
    completeReviewCampaign(facts: {
        remaining: number;
        campaignVersion: number;
        now: number;
        item: IamReviewItemFact;
    }): Promise<void>;
    insertCampaignCompletedEvent(facts: {
        key: string;
        item: IamReviewItemFact;
        now: number;
    }): Promise<void>;
    lockReviewCampaign(facts: {
        id: string;
    }): Promise<IamCampaignFact | null>;
    decidedReviewItems(facts: {
        id: string;
    }): Promise<{
        n: number;
    }>;
    cancelReviewCampaign(facts: {
        version: number;
        now: number;
        id: string;
    }): Promise<void>;
    insertCampaignCancelledEvent(facts: {
        key: string;
        id: string;
        reason: string;
        now: number;
    }): Promise<void>;
    insertIndependentReview(facts: {
        id: string;
        kind: string;
        payload: unknown;
        observedNow: number;
        reason: string;
    }): Promise<void>;
    lockProposedRole(facts: {
        id: string;
    }): Promise<IamRoleFact | null>;
    createProposedRole(facts: {
        id: string;
        input: RoleInput;
    }): Promise<void>;
    nextRoleRevision(facts: {
        id: string;
    }): Promise<{
        next: number;
    }>;
    appendRoleRevision(facts: {
        id: string;
        revision: number;
        input: RoleInput;
        observedNow: number;
    }): Promise<void>;
    affectedRoleUsers(facts: {
        id: string;
    }): Promise<{
        user_id: string;
    }[]>;
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
    function requireHoldingAccessReview(actor: IamActor) {
        if (actor.platform)
            throw ports.error(422, "HOLDING_REVIEW_REQUIRED", "Run access certification inside a company workspace.");
        ports.companyScope(actor);
        ports.authorize(actor, iamPermission(actor, "iam.review"));
    }
    async function startAccessReviewCampaign(actor: IamActor, input: z.infer<typeof accessReviewCampaignInput>, key: string) {
        requireHoldingAccessReview(actor);
        await ports.fresh(actor);
        if (input.dueAt <= ports.now() || input.dueAt > ports.now() + 366 * 86400000)
            throw ports.error(422, "REVIEW_DUE_DATE_INVALID", "Choose a review due date within the next year.");
        return ports.command(actor, key, "access-review.start", input, "iam.review", async (tx) => {
            const assignments = (await tx.activeReviewAssignments({ observedNow: ports.now() }));
            if (!assignments.length)
                throw ports.error(409, "NO_ACTIVE_ASSIGNMENTS", "No active assignments are available for review.");
            const id = `access-review-${key}`, now = ports.now();
            (await tx.insertReviewCampaign({ id, input, now }));
            for (const [index, a] of assignments.entries())
                (await tx.insertReviewItem({ key, index, id, a }));
            (await tx.insertCampaignStartedEvent({ key, id, input, assignments, now }));
            return { id, status: "open", version: 1, itemCount: assignments.length };
        });
    }
    async function decideAccessReviewItem(actor: IamActor, itemId: string, input: z.infer<typeof accessReviewDecisionInput>, key: string) {
        requireHoldingAccessReview(actor);
        await ports.fresh(actor);
        return ports.command(actor, key, "access-review.decide", { itemId, ...input }, "iam.review", async (tx) => {
            const item = (await tx.lockReviewItem({ itemId }));
            if (!item)
                throw ports.error(404, "ACCESS_REVIEW_ITEM_NOT_FOUND", "Access review item not found.");
            if (item.campaign_status !== "open" || item.decision !== "pending")
                fail("ACCESS_REVIEW_ITEM_CHANGED", "This access review item is no longer pending.");
            if (Number(item.campaign_version) !== input.expectedCampaignVersion)
                fail("ACCESS_REVIEW_CAMPAIGN_CHANGED", "The campaign changed. Reload before deciding.");
            if (item.created_by === actor.userId)
                throw ports.error(403, "INDEPENDENT_REVIEW_REQUIRED", "A different administrator must certify the campaign creator's access review.");
            if (item.user_id === actor.userId || (await tx.reviewerSamePerson({ item })).same)
                throw ports.error(403, "SELF_ACCESS_REVIEW", "Another administrator must review your access.");
            const assignment = (await tx.lockReviewAssignment({ item }));
            if (!assignment || Number(assignment.version) !== Number(item.assignment_version) || assignment.revoked_at !== null || !(await tx.reviewAssignmentActive({ item, observedNow: ports.now() })).active)
                fail("REVIEW_SOURCE_CHANGED", "The assignment changed. Start a new access review for current evidence.");
            if (input.decision === "revoke") {
                const protectedMember = (await tx.reviewRecoveryMember({ item }));
                if (protectedMember)
                    ports.authorize(actor, iamPermission(actor, "iam.recovery"));
                await applyIamRevocation(tx, actor, item.assignment_id, Number(item.assignment_version), input.reason, true);
            }
            const now = ports.now();
            (await tx.recordItemDecision({ input, now, itemId }));
            (await tx.insertDecisionEvent({ key, item, itemId, input, now }));
            const remaining = Number((await tx.pendingReviewItems({ item })).n), campaignVersion = Number(item.campaign_version) + 1;
            (await tx.completeReviewCampaign({ remaining, campaignVersion, now, item }));
            if (!remaining)
                (await tx.insertCampaignCompletedEvent({ key, item, now }));
            return { id: itemId, campaignId: item.campaign_id, decision: input.decision, campaignStatus: remaining ? "open" : "completed", campaignVersion };
        });
    }
    async function cancelAccessReviewCampaign(actor: IamActor, id: string, expectedVersion: number, reason: string, key: string) {
        requireHoldingAccessReview(actor);
        await ports.fresh(actor);
        return ports.command(actor, key, "access-review.cancel", { id, expectedVersion, reason }, "iam.review", async (tx) => {
            const campaign = (await tx.lockReviewCampaign({ id }));
            if (!campaign)
                throw ports.error(404, "ACCESS_REVIEW_CAMPAIGN_NOT_FOUND", "Access review campaign not found.");
            if (campaign.created_by !== actor.userId)
                throw ports.error(403, "CAMPAIGN_OWNER_REQUIRED", "Only the campaign creator can cancel it.");
            if (campaign.status !== "open" || Number(campaign.version) !== expectedVersion)
                fail("ACCESS_REVIEW_CAMPAIGN_CHANGED", "The campaign changed. Reload before cancelling.");
            if (Number((await tx.decidedReviewItems({ id })).n))
                throw ports.error(409, "CAMPAIGN_DECISIONS_EXIST", "A campaign with recorded decisions cannot be cancelled.");
            const now = ports.now(), version = expectedVersion + 1;
            (await tx.cancelReviewCampaign({ version, now, id }));
            (await tx.insertCampaignCancelledEvent({ key, id, reason, now }));
            return { id, status: "cancelled", version };
        });
    }
    async function review(tx: IamServiceTransaction, actor: IamActor, kind: string, payload: unknown, reason: string) {
        const id = ports.id();
        (await tx.insertIndependentReview({ id, kind, payload, observedNow: ports.now(), reason }));
        return {
            id,
            status: "pending",
            message: "Submitted for independent review.",
        };
    }
    async function saveIamRole(actor: IamActor, input: z.infer<typeof roleInput>, key: string) {
        ports.companyScope(actor);
        await ports.fresh(actor);
        const catalog = permissionCatalog
            .filter((p) => (p.namespace === "platform") === actor.platform &&
            p.availability === "live")
            .map((p) => p.id);
        if ([...input.permissions, ...input.grantablePermissions].some((p) => !catalog.includes(p) || !actor.iam?.grantablePermissions.includes(p)))
            throw ports.error(403, "DELEGATION_EXCEEDED", "The role exceeds your delegated permission set.");
        if (input.scopes.some((s) => actor.platform ? s !== "platform" : s === "platform"))
            throw ports.error(422, "ROLE_SCOPE_INVALID", "Role scopes must remain inside their namespace.");
        return ports.command(actor, key, "role.propose", input, "iam.roles.manage", async (tx) => {
            if (input.id &&
                !input.id.startsWith(actor.platform ? "platform_custom_" : "custom_"))
                throw ports.error(403, "PROTECTED_ROLE", "Clone a built-in role instead of replacing it.");
            const id = input.id ??
                `${actor.platform ? "platform_custom_" : "custom_"}${ports.id()}`;
            const existing = (await tx.lockProposedRole({ id }));
            if (existing?.built_in)
                fail("PROTECTED_ROLE", "Built-in roles are protected. Clone the role instead.");
            if (Number(existing?.published_revision ?? 0) !== input.expectedVersion)
                fail("ROLE_CHANGED", "The role changed. Review the latest revision.");
            if (!existing)
                (await tx.createProposedRole({ id, input }));
            const revision = Number((await tx.nextRoleRevision({ id })).next);
            (await tx.appendRoleRevision({ id, revision, input, observedNow: ports.now() }));
            const affected = (await tx.affectedRoleUsers({ id })).map((r) => r.user_id);
            return {
                ...(await review(tx, actor, "role.publish", {
                    id,
                    name: input.name,
                    revision,
                    expectedVersion: input.expectedVersion,
                    permissions: input.permissions,
                    grantablePermissions: input.grantablePermissions,
                    affected,
                }, input.reason)),
                roleId: id,
                revision,
                affected,
            };
        });
    }
    async function validateIamGrant(tx: IamServiceTransaction, actor: IamActor, input: z.infer<typeof grantInput>, parentId = actor.assignmentId) {
        if (input.userId === actor.userId ||
            (await tx.grantSamePerson({ input })).same)
            throw ports.error(403, "SELF_ACCESS_CHANGE", "Another administrator must change your access.");
        const parent = (await tx.lockGrantParent({ parentId }));
        if (!parent ||
            !(await tx.grantParentActive({ parentId, observedNow: ports.now() })).ok)
            fail("PARENT_EXPIRED", "The authorizing assignment is no longer active.");
        const reviewer = (await tx.grantReviewerExpiry({}));
        if (input.validUntil === 0)
            input.validUntil = Math.min(ports.now() + 90 * 86400000, Number(parent.valid_until), Number(reviewer?.valid_until ?? parent.valid_until));
        if (input.validUntil <= ports.now() ||
            input.validUntil > Number(parent.valid_until) ||
            (reviewer && input.validUntil > Number(reviewer.valid_until)))
            throw ports.error(422, "EXPIRY_INVALID", "Access must expire within the authorizing assignment.");
        const parentRole = (await tx.grantParentPermissions({ parent }));
        if (!parentRole)
            fail("PARENT_ROLE_UNAVAILABLE", "The authorizing role is unavailable.");
        const member = (await tx.grantTargetMember({ input }));
        if (input.userId.startsWith("dir_") && member?.login_policy !== "directory")
            throw ports.error(403, "DIRECTORY_IDENTITY_REVIEW_REQUIRED", "Connect and review this directory identity in this company before assigning access.");
        if (member && member.status !== "active")
            throw ports.error(403, "IDENTITY_INACTIVE", "Suspended or superseded identities cannot receive new access.");
        const role = (await tx.grantPublishedRole({ input }));
        if (!role || !role.scopes_json.includes(input.scopeKind))
            fail("ROLE_UNAVAILABLE", "Choose an active role and supported scope.");
        if (input.expectedRoleRevision !== undefined &&
            input.expectedRoleRevision !== Number(role.published_revision))
            fail("ROLE_CHANGED", "The role changed. Review its current permissions before granting access.");
        input.expectedRoleRevision = Number(role.published_revision);
        if ([...role.permissions_json, ...role.grantable_json].some((p: string) => !actor.iam?.grantablePermissions.includes(p) ||
            !parentRole.grantable_json.includes(p)))
            throw ports.error(403, "DELEGATION_EXCEEDED", "This role exceeds your delegated grant permissions.");
        if (!actor.platform) {
            if (parent.company_id !== actor.companyId)
                throw ports.error(403, "SCOPE_FORBIDDEN", "The parent belongs to another company.");
            if (input.scopeKind === "platform")
                throw ports.error(403, "SCOPE_FORBIDDEN", "Platform access is separate.");
            if (input.scopeKind !== "holding") {
                const node = (await tx.grantScopeNode({ input }));
                if (!node)
                    fail("SCOPE_UNAVAILABLE", "Choose an active organization scope.");
                if ((parent.scope_kind === "department" &&
                    parent.scope_node_id !== node.id) ||
                    (parent.scope_kind === "branch" &&
                        parent.scope_node_id !==
                            (node.kind === "branch" ? node.id : node.branch_id)) ||
                    (actor.scopeKind === "department" && actor.scopeNodeId !== node.id) ||
                    (actor.scopeKind === "branch" &&
                        actor.scopeNodeId !==
                            (node.kind === "branch" ? node.id : node.branch_id)))
                    throw ports.error(403, "SCOPE_FORBIDDEN", "The grant exceeds its parent scope.");
            }
            else if (parent.scope_kind !== "holding" || actor.scopeKind !== "holding")
                throw ports.error(403, "SCOPE_FORBIDDEN", "The grant exceeds its parent scope.");
        }
        return role;
    }
    async function insertIamGrant(tx: IamServiceTransaction, actor: IamActor, input: z.infer<typeof grantInput>, parentId: string, source = "manual", sourceId: string | null = null) {
        const id = `iam-${ports.id()}`, modules = [
            ...new Set(input.roleId === "holding_owner"
                ? permissionCatalog
                    .filter((p) => p.id.startsWith("module."))
                    .map((p) => p.module)
                : (await tx.grantModulePermissions({ input })).flatMap((r) => (r.permissions_json as string[])
                    .filter((p) => p.startsWith("module."))
                    .map((p) => p.split(".")[1]))),
        ];
        if (actor.platform)
            (await tx.insertPlatformGrant({ id, input, modules, observedNow: ports.now(), parentId, source, sourceId }));
        else
            (await tx.insertCompanyGrant({ id, input, modules, observedNow: ports.now(), parentId, source, sourceId }));
        return id;
    }
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
    return { startAccessReviewCampaign, decideAccessReviewItem, cancelAccessReviewCampaign, review, saveIamRole, validateIamGrant, insertIamGrant, grantIamAccess, revokeIamAssignment, applyIamRevocation, decideIamReview };
}
