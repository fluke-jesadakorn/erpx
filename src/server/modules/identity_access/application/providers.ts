import type { ProviderConnection, ProviderSnapshot, DirectorySecret, ProviderFailure } from '../domain/providers';
import { createProviderPolicy } from '../domain/providers';
export interface DirectoryProviderPorts {
    error: ProviderFailure;
    read(connection: ProviderConnection): Promise<ProviderSnapshot>;
    secret(connection: ProviderConnection): Promise<DirectorySecret>;
    discovery(url: string): Promise<{
        issuer?: string;
        jwks_uri?: string;
        token_endpoint?: string;
        authorization_endpoint?: string;
    }>;
    verify(token: string, jwks: string, issuer: string, clientId: string): Promise<Record<string, unknown>>;
}
export function createTestDirectory(ports: DirectoryProviderPorts) {
    const { approvedUrl } = createProviderPolicy(ports.error);
    return async (connection: ProviderConnection) => {
        const snapshot = await ports.read(connection);
        if (connection.config.mode === "oidc") {
            const secret = await ports.secret(connection), issuer = connection.config.issuer;
            approvedUrl(issuer, secret.allowedHosts, "https:");
            if (!secret.clientSecret || !connection.config.clientId)
                throw ports.error(422, "OIDC_CONFIGURATION_INVALID", "A client ID and external client-secret profile are required.");
            const discovery = await ports.discovery(`${issuer!.replace(/\/$/, "")}/.well-known/openid-configuration`);
            if (discovery.issuer !== issuer ||
                !discovery.jwks_uri ||
                !discovery.token_endpoint)
                throw ports.error(502, "OIDC_DISCOVERY_INVALID", "The issuer did not return valid OIDC discovery.");
            for (const endpoint of [
                discovery.jwks_uri,
                discovery.token_endpoint,
                discovery.authorization_endpoint,
            ])
                approvedUrl(endpoint, secret.allowedHosts, "https:");
            if (connection.provider === "univention") {
                if (!secret.verificationIdToken)
                    throw ports.error(409, "UCS_IDENTITY_MAPPING_REQUIRED", "Install a current Keycloak verification ID token in the external test profile to prove the immutable entryUUID mapping.");
                try {
                    const payload = await ports.verify(secret.verificationIdToken, discovery.jwks_uri, issuer!, connection.config.clientId!);
                    if (typeof payload.entryUUID !== "string" ||
                        !payload.sub ||
                        !snapshot.users.some((u) => u.externalId === payload.entryUUID))
                        throw new Error("Invalid mapping");
                }
                catch {
                    throw ports.error(409, "UCS_IDENTITY_MAPPING_INVALID", "The Keycloak test identity must have a verified immutable entryUUID matching the LDAP directory.");
                }
            }
        }
        return {
            userCount: snapshot.users.length,
            groupCount: snapshot.groups.length,
            snapshot,
        };
    };
}
export function createDirectoryReader(ports: {
    entra(connection: ProviderConnection): Promise<ProviderSnapshot>;
    ldap(connection: ProviderConnection): Promise<ProviderSnapshot>;
}) { return (connection: ProviderConnection) => connection.provider === 'entra_id' ? ports.entra(connection) : ports.ldap(connection); }
