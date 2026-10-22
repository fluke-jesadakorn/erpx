import { validRecoveryKey, type EmergencyActor, type RecoveryScope, type RecoveryInput } from '@erpx/server-identity_access/domain/recovery';
import type { IdentityError } from '@erpx/server-identity_access/application/authorization';
export function recoveryWorkspaceScope(input: RecoveryInput, holding: {
    slug: string;
    displayName: string;
} | null, error: IdentityError): RecoveryScope { if (!holding && input.companyId !== 'platform')
    throw error(422, 'WORKSPACE_REQUIRED', 'Choose the company and holding for emergency access.'); return { companyId: input.companyId, holdingSlug: holding?.slug ?? null, scopeLabel: holding ? `${holding.displayName} · ${input.companyId}` : 'Platform administration' }; }
export type RecoveryResult = Record<string, unknown>;
export interface RecoveryTransaction {
    receipt(): Promise<{
        company_id: string;
        actor_id: string;
        request_hash: string;
        result: RecoveryResult;
    } | null>;
    freshMfa(): Promise<void>;
    member(): Promise<boolean>;
    grant(): Promise<{
        valid_until: number;
    } | null>;
    activate(expiresAt: number): Promise<void>;
    revoke(): Promise<boolean>;
    audit(operation: string, detail: Record<string, unknown>): Promise<void>;
    record(result: RecoveryResult): Promise<void>;
}
export interface RecoveryCommandPorts {
    error: IdentityError;
    scope(input: RecoveryInput): Promise<RecoveryScope>;
    transaction(scope: RecoveryScope, actor: EmergencyActor, key: string, operation: string, input: RecoveryInput, run: (tx: RecoveryTransaction, now: number, digest: string) => Promise<RecoveryResult>): Promise<RecoveryResult>;
}
export function createRecoveryCommands(ports: RecoveryCommandPorts) { return async (operation: 'emergency.activate' | 'emergency.revoke', actor: EmergencyActor, input: RecoveryInput, key: string) => { if (!validRecoveryKey(key))
    throw ports.error(422, 'COMMAND_KEY_REQUIRED', 'A valid Idempotency-Key is required.'); const scope = await ports.scope(input); return ports.transaction(scope, actor, key, operation, input, async (tx, now, digest) => { const receipt = await tx.receipt(); if (receipt) {
    if (receipt.company_id !== scope.companyId || receipt.actor_id !== actor.userId || receipt.request_hash !== digest)
        throw ports.error(409, 'IDEMPOTENCY_CONFLICT', 'This request key was already used for a different emergency access action.');
    return receipt.result;
} if (operation === 'emergency.activate') {
    await tx.freshMfa();
    const member = await tx.member(), grant = await tx.grant();
    if (!member || !grant)
        throw ports.error(403, 'RECOVERY_UNAVAILABLE', 'This account is not a designated recovery administrator in this workspace.');
    const expiresAt = Math.min(now + 3600000, Number(grant.valid_until));
    await tx.activate(expiresAt);
    const result = { expiresAt, message: 'Emergency access activated for up to one hour.' };
    await tx.audit('emergency.activated', { reason: input.reason, expiresAt });
    await tx.record(result);
    return result;
} const revoked = await tx.revoke(); if (!revoked)
    throw ports.error(404, 'EMERGENCY_ACCESS_NOT_ACTIVE', 'No active temporary access exists for this session and workspace.'); const result = { revoked: true, message: 'Temporary emergency access was revoked for this session.' }; await tx.audit('emergency.revoked', { reason: input.reason, revoked }); await tx.record(result); return result; }); }; }
export interface RecoveryReadPorts {
    now(): number;
    error: IdentityError;
    platform(): Promise<RecoveryScope>;
    holdings(): Promise<{
        id: string;
        slug: string;
        displayName: string;
    }[]>;
    holding(holding: {
        id: string;
        slug: string;
        displayName: string;
    }): Promise<RecoveryScope>;
    sessions(scope: RecoveryScope, actor: EmergencyActor, now: number): Promise<{
        company_id: string;
        expires_at: number;
        reason: string;
    }[]>;
}
export function createEmergencyAccessList(ports: RecoveryReadPorts) { return async (actor: EmergencyActor) => { const now = ports.now(), access: Array<{
    companyId: string;
    holdingSlug: string | null;
    scopeLabel: string;
    reason: string;
    expiresAt: number;
}> = [], unavailableScopes: string[] = [], scopes = [await ports.platform()]; for (const holding of await ports.holdings())
    try {
        scopes.push(await ports.holding(holding));
    }
    catch {
        unavailableScopes.push(holding.displayName);
    } for (const scope of scopes)
    try {
        for (const row of await ports.sessions(scope, actor, now))
            access.push({ companyId: row.company_id, holdingSlug: scope.holdingSlug, scopeLabel: scope.holdingSlug === null ? scope.scopeLabel : scope.scopeLabel + ' · ' + row.company_id, reason: row.reason, expiresAt: Number(row.expires_at) });
    }
    catch {
        if (scope.holdingSlug)
            unavailableScopes.push(scope.scopeLabel);
        else
            throw ports.error(503, 'RECOVERY_STATUS_UNAVAILABLE', 'Temporary access status is currently unavailable. Try again shortly.');
    } return { access, unavailableScopes }; }; }
