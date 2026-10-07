import type { IdentityError } from './authorization';
export function createRefreshDirectoryMember<Connection>(ports: {
    stale(companyId: string, userId: string, cutoff: number): Promise<Connection | null>;
    synchronize(row: Connection): Promise<void>;
    now(): number;
    error: IdentityError;
}) { return async (companyId: string, userId: string) => { const row = await ports.stale(companyId, userId, ports.now() - 300000); if (row)
    try {
        await ports.synchronize(row);
    }
    catch {
        throw ports.error(403, 'DIRECTORY_ACCESS_UNAVAILABLE', 'Directory verification failed. Dependent access is unavailable until a successful refresh.');
    } }; }
