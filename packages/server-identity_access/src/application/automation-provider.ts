import type { DirectoryAction, DirectoryInventory } from "@erpx/feature-identity_access/lib/directory-operations";
import { compileAdministrativePolicies } from "@erpx/feature-identity_access/lib/directory-operations";
import { compilePreferences } from "@erpx/feature-shared/lib/windows-preferences";
import { createDirectoryPolicy, directoryActionTargets } from '@erpx/server-identity_access/domain/directory-provider';
import type { DirectorySecret, ProviderConnection } from '@erpx/server-identity_access/domain/providers';
import type { IdentityError } from '@erpx/server-identity_access/application/authorization';
export interface AutomationProviderPorts {
    error: IdentityError;
    secret(connection: ProviderConnection): Promise<DirectorySecret>;
    windowsInventory(connection: ProviderConnection): Promise<unknown>;
    univentionInventory(connection: ProviderConnection): Promise<DirectoryInventory>;
    runnerApply(connection: ProviderConnection, payload: {
        connectionId: string;
        operationId: string;
        action: DirectoryAction;
        before: DirectoryInventory;
        registry: ReturnType<typeof compileAdministrativePolicies>;
        preferences: ReturnType<typeof compilePreferences>;
    }): Promise<Record<string, unknown>>;
    udmApply(connection: ProviderConnection, operationId: string, action: DirectoryAction, before: DirectoryInventory): Promise<Record<string, unknown>>;
}
export function createAutomationProvider(ports: AutomationProviderPorts) {
    const policy = createDirectoryPolicy(ports.error);
    async function readAutomationInventory(connection: ProviderConnection): Promise<DirectoryInventory> { if (connection.provider === 'active_directory') {
        const secret = await ports.secret(connection);
        return policy.canonicalInventory(policy.parsedInventory(await ports.windowsInventory(connection)), secret.identityAliases);
    } if (connection.provider !== 'univention')
        throw ports.error(409, 'PROVIDER_UNSUPPORTED', 'This connector does not support directory preparation.'); return ports.univentionInventory(connection); }
    async function applyDirectoryAction(connection: ProviderConnection, operationId: string, action: DirectoryAction, before: DirectoryInventory) { const secret = await ports.secret(connection); if (!secret.writeOperations.includes(action.kind))
        throw ports.error(403, 'DIRECTORY_WRITE_NOT_APPROVED', 'This operation is not allowed by the installed provider profile.'); policy.validateObjectTarget(action, before); policy.assertWriteBoundary(secret, directoryActionTargets(action, before)); if (connection.provider === 'active_directory' || ['gpo.publish', 'gpo.link', 'gpo.filter', 'gpo.order', 'gpo.inheritance'].includes(action.kind))
        return ports.runnerApply(connection, { connectionId: connection.id, operationId, action, before, registry: action.kind === 'gpo.publish' ? compileAdministrativePolicies(action.definition) : [], preferences: action.kind === 'gpo.publish' ? compilePreferences(action.definition, new Date(before.observedAt).toISOString().slice(0, 19).replace('T', ' ')) : [] }); return ports.udmApply(connection, operationId, action, before); }
    return { readAutomationInventory, applyDirectoryAction };
}
