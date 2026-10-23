import type {ConnectionRow} from '@erpx/server-identity_access/domain/directories';
import type {IdentityError} from '@erpx/server-identity_access/application/authorization';
export function createConnectionLookup(ports: {
    lookup(companyId: string, id: string): Promise<ConnectionRow | null>;
    error: IdentityError;
}) { return async (companyId: string, id: string) => { const row = await ports.lookup(companyId, id); if (!row)
    throw ports.error(404, 'CONNECTION_NOT_FOUND', 'Directory connection not found.'); return row; }; }
export function requireDirectoryRoute<T>(route: T | null, error: IdentityError): T {
    if (!route) throw error(404, "CONNECTION_NOT_FOUND", "Directory connection not found.");
    return route;
}
