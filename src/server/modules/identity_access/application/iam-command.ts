import type { IamActor } from "../domain/iam-actor";
import { iamPermission } from "../domain/iam-actor";
import { iamCommandKeyValid, iamCommandNeedsFreshMfa, type IamCommandAssignment, type IamCommandRolePolicy, type IamCommandMember, type IamCommandDirectoryIdentity, type IamCommandWorkContext, type IamCommandReceipt } from "../domain/command-policy";
import type { IdentityError } from "./authorization";
export type IamCommandRequest = {
    actor: IamActor;
    key: string;
    operation: string;
    payload: unknown;
    permission: string;
    digest: string;
};
export interface IamCommandTransaction<T> {
    assignment(actor: IamActor): Promise<IamCommandAssignment | null>;
    active(assignmentId: string, now: number): Promise<boolean>;
    allowed(role: string, companyId: string, permission: string): Promise<boolean>;
    rolePolicy(role: string, companyId: string): Promise<IamCommandRolePolicy | null>;
    member(actor: IamActor): Promise<IamCommandMember | null>;
    directoryIdentity(actor: IamActor, member: IamCommandMember): Promise<IamCommandDirectoryIdentity | null>;
    revoked(actor: IamActor): Promise<boolean>;
    workContext(actor: IamActor): Promise<IamCommandWorkContext | null>;
    receipt(key: string): Promise<IamCommandReceipt | null>;
    recovery(actor: IamActor, now: number): Promise<boolean>;
    perform(): Promise<T>;
    freshMfa(actor: IamActor): Promise<void>;
    recordReceipt(request: IamCommandRequest, result: T, now: number): Promise<void>;
    recordAudit(request: IamCommandRequest, result: T, now: () => number): Promise<void>;
}
export function createIamCommand<T>(ports: {
    error: IdentityError;
    now(): number;
    digest(value: string): string;
    transaction(actor: IamActor, run: (tx: IamCommandTransaction<T>) => Promise<T>): Promise<T>;
}) {
    return async (actor: IamActor, key: string, operation: string, payload: unknown, permission: string): Promise<T> => {
        if (!iamCommandKeyValid(key))
            throw ports.error(422, 'COMMAND_KEY_REQUIRED', 'A valid Idempotency-Key is required.');
        const digest = ports.digest(JSON.stringify({ actor: actor.userId, assignment: actor.assignmentId, company: actor.companyId, operation, payload }));
        return ports.transaction(actor, async (tx) => {
            const assignment = await tx.assignment(actor);
            if (!assignment || !await tx.active(actor.assignmentId, ports.now()))
                throw ports.error(403, 'ASSIGNMENT_UNAVAILABLE', 'Your authorizing assignment is no longer active.');
            if (!await tx.allowed(assignment.role, actor.companyId, iamPermission(actor, permission)))
                throw ports.error(403, 'PERMISSION_REQUIRED', 'Your selected role does not grant this operation.');
            const policy = await tx.rolePolicy(assignment.role, actor.companyId);
            if (!policy || actor.iam && Number(policy.published_revision) !== actor.iam.roleRevision || assignment.role !== actor.role)
                throw ports.error(409, 'POLICY_CHANGED', 'The role policy changed. Refresh and review again.');
            actor = { ...actor, iam: actor.iam ? { ...actor.iam, grantablePermissions: policy.grantable_json } : undefined };
            const member = await tx.member(actor);
            if (actor.iam && Number(member?.version ?? 0) !== actor.iam.identityVersion)
                throw ports.error(409, 'IDENTITY_CHANGED', 'Your identity changed. Refresh first.');
            if (member && member.status !== 'active')
                throw ports.error(403, 'IDENTITY_SUSPENDED', 'Your identity is inactive.');
            if (member?.login_policy === 'directory') {
                const identity = await tx.directoryIdentity(actor, member);
                if (!identity?.enabled || identity.status !== 'active' || Number(identity.observed_at) <= ports.now() - 300000)
                    throw ports.error(403, 'DIRECTORY_ACCESS_UNAVAILABLE', 'Directory access requires a successful refresh.');
            }
            if (await tx.revoked(actor))
                throw ports.error(401, 'SESSION_REVOKED', 'Sign in again to continue.');
            if (member?.login_policy === 'emergency' && (!actor.iam?.emergencyUntil || actor.iam.emergencyUntil <= ports.now()))
                throw ports.error(403, 'EMERGENCY_ACCESS_REQUIRED', 'Activate a temporary emergency session.');
            if (!actor.platform) {
                const context = await tx.workContext(actor);
                if (!context || actor.authority && Number(context.revision) !== actor.authority.revision || context.assignment_id !== actor.assignmentId || Number(context.assignment_version) !== Number(assignment.version))
                    throw ports.error(409, 'WORK_CONTEXT_CHANGED', 'Choose your current work role again.');
            }
            const old = await tx.receipt(key);
            if (old) {
                if (old.request_hash !== digest)
                    throw ports.error(409, 'IDEMPOTENCY_CONFLICT', 'This key belongs to another request.');
                return old.result as T;
            }
            const hadRecovery = await tx.recovery(actor, ports.now()), result = await tx.perform();
            if (hadRecovery && !await tx.recovery(actor, ports.now()))
                throw ports.error(409, 'LAST_RECOVERY_ADMINISTRATOR', 'The final recovery administrator must retain active access.');
            if (iamCommandNeedsFreshMfa(operation))
                await tx.freshMfa(actor);
            const request: IamCommandRequest = { actor, key, operation, payload, permission, digest };
            await tx.recordReceipt(request, result, ports.now());
            await tx.recordAudit(request, result, ports.now);
            return result;
        });
    };
}
