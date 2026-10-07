import { betaDestinationAllowed } from "../domain/beta-access";
import type { IdentityError } from "./authorization";
export function createBetaAccess(ports: {
    enabled(companyId: string): Promise<boolean>;
    error: IdentityError;
}) { return async (companyId: string, requestHeaders: Headers) => { const beta = await ports.enabled(companyId), destination = requestHeaders.get('x-erpx-destination'); if (beta && destination && !betaDestinationAllowed(destination, requestHeaders.get('x-erpx-api') === 'true'))
    throw ports.error(403, 'BETA_CAPABILITY_UNAVAILABLE', 'This capability is not part of the public beta.'); return beta; }; }
