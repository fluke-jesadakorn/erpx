import type { IdentityError } from "@erpx/server-identity_access/application/authorization";
import { authenticatorCodeValid, authenticatorCounters } from "@erpx/server-identity_access/domain/mfa";
export type MfaActor = {
    userId: string;
    sessionId: string;
};
export function createRequireFreshMfa(ports: {
    ready(): Promise<void>;
    now(): number;
    isoNow(): string;
    error: IdentityError;
    verified(actor: MfaActor, cutoff: number, isoNow: string): Promise<boolean>;
}) { return async (actor: MfaActor) => { await ports.ready(); if (!await ports.verified(actor, ports.now() - 300000, ports.isoNow()))
    throw ports.error(403, 'FRESH_MFA_REQUIRED', 'Verify an authenticator code in My access before this sensitive change.'); }; }
export function createBeginMfa(ports: {
    throttle(actor: MfaActor): Promise<void>;
    enabled(actor: MfaActor): Promise<boolean>;
    sessionCreated(actor: MfaActor): Promise<string | null>;
    now(): number;
    error: IdentityError;
    secret(): string;
    seal(secret: string): Promise<string>;
    enroll(actor: MfaActor, sealed: string): Promise<void>;
}) { return async (actor: MfaActor) => { await ports.throttle(actor); if (await ports.enabled(actor))
    throw ports.error(409, 'MFA_ALREADY_ENABLED', 'Your authenticator is already configured.'); const created = await ports.sessionCreated(actor); if (!created || !Number.isFinite(Date.parse(created)) || Date.parse(created) < ports.now() - 300000)
    throw ports.error(403, 'REAUTHENTICATION_REQUIRED', 'Sign in again before enrolling an authenticator.'); const secret = ports.secret(); await ports.enroll(actor, await ports.seal(secret)); return { secret, uri: `otpauth://totp/ERPX:${encodeURIComponent(actor.userId)}?secret=${secret}&issuer=ERPX&algorithm=SHA1&digits=6&period=30` }; }; }
export function createVerifyMfa(ports: {
    throttle(actor: MfaActor): Promise<void>;
    enrollment(actor: MfaActor): Promise<{
        secret: string;
        last_counter: number;
    } | null>;
    now(): number;
    error: IdentityError;
    open(secret: string): Promise<string>;
    matches(secret: string, counter: number, code: string): boolean;
    activate(actor: MfaActor, counter: number): Promise<boolean>;
    verified(actor: MfaActor, now: number): Promise<void>;
}) { return async (actor: MfaActor, code: string) => { await ports.throttle(actor); if (!authenticatorCodeValid(code))
    throw ports.error(422, 'MFA_INVALID', 'Enter a six-digit authenticator code.'); const row = await ports.enrollment(actor); if (!row)
    throw ports.error(409, 'MFA_NOT_ENROLLED', 'Set up your authenticator first.'); const now = Math.floor(ports.now() / 30000), secret = await ports.open(row.secret), counter = authenticatorCounters(now).find(c => c > Number(row.last_counter) && ports.matches(secret, c, code)); if (counter === undefined)
    throw ports.error(403, 'MFA_INVALID', 'The code is invalid or already used.'); if (!await ports.activate(actor, counter))
    throw ports.error(403, 'MFA_REPLAY', 'This code was already used.'); await ports.verified(actor, ports.now()); return { verifiedUntil: ports.now() + 300000 }; }; }
