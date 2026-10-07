import { directoryClaimsEligible } from '../domain/auth';
export type DirectoryAuthFailure = (code: 'FORBIDDEN' | 'UNAUTHORIZED', message: string) => Error;
export type DirectoryAuthConnection = {
    id: string;
    status: string;
    config: {
        mode: string;
    };
};
export function createDirectoryUser<Connection extends DirectoryAuthConnection>(ports: {
    connection(id: string): Promise<Connection>;
    synchronize(connection: Connection): Promise<void>;
    identity(connectionId: string, externalId: string, cutoff: number): Promise<{
        user_id: string;
    } | null>;
    now(): number;
    error: DirectoryAuthFailure;
}) { return async (connectionId: string, externalId: string) => { const connection = await ports.connection(connectionId); if (connection.status !== 'active')
    throw ports.error('FORBIDDEN', 'Directory sign-in is unavailable.'); await ports.synchronize(connection); const identity = await ports.identity(connectionId, externalId, ports.now() - 300000); if (!identity)
    throw ports.error('FORBIDDEN', 'Your directory identity has not been approved for ERPX access.'); return identity.user_id; }; }
export function createDirectorySignIn<Connection extends DirectoryAuthConnection, User extends {
    id: string;
}, Session>(ports: {
    throttle(connectionId: string, username: string): Promise<void>;
    connection(id: string): Promise<Connection>;
    verify(connection: Connection, username: string, password: string): Promise<string>;
    approved(connectionId: string, externalId: string): Promise<string>;
    user(id: string): Promise<User | null>;
    session(userId: string): Promise<Session | null>;
    error: DirectoryAuthFailure;
}) { return async (input: {
    connectionId: string;
    username: string;
    password: string;
}) => { await ports.throttle(input.connectionId, input.username); try {
    const connection = await ports.connection(input.connectionId);
    if (connection.status !== 'active' || connection.config.mode !== 'ldaps')
        throw new Error('Directory unavailable');
    const externalId = await ports.verify(connection, input.username, input.password), userId = await ports.approved(connection.id, externalId), user = await ports.user(userId);
    if (!user)
        throw new Error('Account unavailable');
    const session = await ports.session(user.id);
    if (!session)
        throw new Error('Session unavailable');
    return { session, user };
}
catch {
    throw ports.error('UNAUTHORIZED', 'Directory sign-in failed. Check your credentials and approved access.');
} }; }
export function createDirectoryOidcUserInfo(ports: {
    decode(token: string): Record<string, unknown>;
    bind(subject: string, externalId: string): Promise<boolean>;
    approved(externalId: string): Promise<string>;
    user(userId: string): Promise<{
        name: string;
        email: string;
    } | null>;
}, provider: string, issuer: string, tenantId?: string) { return async (token: string | undefined) => { if (!token)
    return null; const claims = ports.decode(token); if (!directoryClaimsEligible(provider, claims, issuer, tenantId))
    return null; const externalId = provider === 'entra_id' ? claims.oid : claims.entryUUID; if (typeof externalId !== 'string' || !externalId)
    return null; if (!claims.sub)
    return null; if (!await ports.bind(String(claims.sub), externalId))
    return null; const user = await ports.user(await ports.approved(externalId)); if (!user)
    return null; return { ...claims, id: externalId, name: user.name, email: user.email, emailVerified: true }; }; }
