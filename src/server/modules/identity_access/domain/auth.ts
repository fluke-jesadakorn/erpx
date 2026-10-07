export type AuthOriginPolicy = {
    local: boolean;
    defaultOrigin: string;
    pmsOrigin?: string;
    posOrigin?: string;
    beta: boolean;
    legalInvitationEmail?: string;
};
export function authPolicy(policy: AuthOriginPolicy, requestOrigin?: string) { const allowedOrigins = [policy.defaultOrigin, policy.pmsOrigin, policy.posOrigin].filter((origin): origin is string => typeof origin === 'string' && !!origin), baseURL = requestOrigin && (policy.local || allowedOrigins.includes(requestOrigin)) ? requestOrigin : policy.defaultOrigin, cookiePrefix = baseURL === policy.pmsOrigin ? 'erpx-pms' : baseURL === policy.posOrigin ? 'erpx-pos' : 'better-auth'; return { baseURL, cookiePrefix, allowedOrigins, disableSignUp: !policy.beta && !policy.legalInvitationEmail, requireEmailVerification: !policy.local || policy.beta || !!policy.legalInvitationEmail, sendOnSignUp: policy.beta || !!policy.legalInvitationEmail, trustedOrigins: [...new Set([baseURL, ...allowedOrigins]), ...(policy.local ? ['http://localhost:3100', 'http://127.0.0.1:3100', 'http://localhost:8787', 'http://127.0.0.1:8787'] : [])], verificationProduct: cookiePrefix === 'erpx-pms' ? 'PMS Front' : cookiePrefix === 'erpx-pos' ? 'POS Front' : 'ERPX' }; }
export const localCredentialAllowed = (userId: string) => !userId.startsWith('dir_');
export const legalInvitationAccountAllowed = (email: string, invitationEmail: string) => email.toLowerCase() === invitationEmail.toLowerCase();
export function directoryClaimsEligible(provider: string, claims: Record<string, unknown>, issuer: string, tenantId?: string) { return claims.iss === issuer && (provider !== 'entra_id' || String(claims.tid).toLowerCase() === tenantId?.toLowerCase()); }
export const directorySubject = (provider: string, profile: Record<string, unknown>) => String(provider === 'entra_id' ? (profile.oid ?? '') : (profile.entryUUID ?? ''));
