import { nativePrepare } from '@erpx/server-identity_access/public/native-identity-write';
import { userAttributeByIdExtendedSql } from './identity-user-projection-family-r21';
import "server-only";
import { createCipheriv, createDecipheriv, createHash, createHmac, randomBytes, } from "node:crypto";
import type { User } from "better-auth";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import { sendBetaEmail } from "@erpx/server-manage_erpx/public/beta-email";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
type RecoveryEnv = BetaEnvironment & {
    BETTER_AUTH_SECRET: string;
    CONTROL_DB?: SqlDatabase;
};
type RecoverySender = (env: RecoveryEnv, to: string, subject: string, url: string, key?: string) => Promise<string | null | void>;
import { createAccountRecoveryApplication, type AccountRecoveryRepository, type AccountRecoveryPorts } from '@erpx/server-identity_access/application/account-recovery';
import type { RecoveryDeliveryJob } from '@erpx/server-identity_access/domain/account-recovery';
import { accountRecoveryDeliveryUrl } from "@erpx/feature-runtime/lib/application-origins";
function encryptionKey(secret: string) {
    return createHash("sha256")
        .update(secret)
        .update("erpx-account-recovery-v1")
        .digest();
}
function seal(secret: string, value: string) {
    const iv = randomBytes(12);
    const cipher = createCipheriv("aes-256-gcm", encryptionKey(secret), iv);
    const data = Buffer.concat([cipher.update(value, "utf8"), cipher.final()]);
    return [iv, cipher.getAuthTag(), data]
        .map((part) => part.toString("base64url"))
        .join(".");
}
function open(secret: string, value: string) {
    const [iv, tag, data] = value
        .split(".")
        .map((part) => Buffer.from(part, "base64url"));
    const decipher = createDecipheriv("aes-256-gcm", encryptionKey(secret), iv);
    decipher.setAuthTag(tag);
    return Buffer.concat([decipher.update(data), decipher.final()]).toString("utf8");
}
function repository(control: SqlDatabase): AccountRecoveryRepository {
    const event = (id: string, userId: string, jobId: string | null, action: string, evidence: Record<string, unknown>, now: number) => nativePrepare("w_c26b534670da").bind(id, userId, jobId, action, JSON.stringify(evidence), now);
    return {
        async queue(input) {
            const { id, userId, verificationIdentifier, urlCiphertext, now, expiresAt, eventId } = input;
            await Promise.all([
                nativePrepare("w_b72d2bff48d2").bind(userId, verificationIdentifier),
                nativePrepare("w_ab573fab46e3").bind(now, userId),
                nativePrepare("w_aec55ca072d1").bind(id, userId, verificationIdentifier, urlCiphertext, now, expiresAt, now, now), event(eventId, userId, id, 'requested', { expiresAt }, now)
            ]);
        },
        async recoverInterrupted(now) { await nativePrepare("w_77aad90d676e").bind(now, now, now).run(); },
        claim: (lease, now) => nativePrepare("w_b56d93903546").bind(lease, now + 30000, now, now).first<RecoveryDeliveryJob>(),
        account: userId => control.prepare(userAttributeByIdExtendedSql('email')).bind(userId).first<{
            email: string;
        }>(),
        async cancel(job, lease, reason, now, eventId) { await Promise.all([nativePrepare("w_af917c230df0").bind(reason, now, job.id, lease), event(eventId, job.user_id, job.id, 'cancelled', { reason }, now)]); },
        async sent(job, lease, providerId, now, eventId) { await Promise.all([nativePrepare("w_f77801fb504a").bind(now, providerId ?? job.id, now, job.id, lease), event(eventId, job.user_id, job.id, 'sent', { providerId }, now)]); },
        async failed(job, lease, failure, now, eventId) { await Promise.all([nativePrepare("w_75e34d6665ef").bind(failure.state, failure.code, failure.nextAttemptAt, now, job.id, lease), event(eventId, job.user_id, job.id, 'delivery_failed', { attempt: job.attempts, terminal: failure.terminal, error: failure.code }, now)]); },
        async complete(userId, now, eventId) {
            const latest = await control.prepare('SELECT id FROM account_recovery_delivery_jobs WHERE user_id=? ORDER BY created_at DESC,id DESC LIMIT 1').bind(userId).first<string>('id');
            await Promise.all([
                nativePrepare("w_13a241f7e1ff").bind(userId),
                nativePrepare("w_6d14d44114fd").bind(now, userId), event(eventId, userId, latest ?? null, 'completed', { sessionsRevoked: true }, now)
            ]);
        },
    };
}
function application(control: SqlDatabase, env: RecoveryEnv, send: RecoverySender, now: number) {
    const ports: AccountRecoveryPorts = { repository: repository(control), clock: { now: () => now }, identifiers: { next: () => crypto.randomUUID() }, productOrigins: { pms: process.env.ERPX_FRONT_PMS_ORIGIN, pos: process.env.ERPX_FRONT_POS_ORIGIN }, cryptography: { recoveryId: token => `account-recovery-${createHmac('sha256', env.BETTER_AUTH_SECRET).update(token).digest('hex').slice(0, 40)}`, sealLink: url => seal(env.BETTER_AUTH_SECRET, accountRecoveryDeliveryUrl(url)), openLink: ciphertext => accountRecoveryDeliveryUrl(open(env.BETTER_AUTH_SECRET, ciphertext)) }, send: (to, subject, url, key) => send(env, to, subject, url, key) };
    return createAccountRecoveryApplication(ports);
}
export async function queueAccountRecovery(control: SqlDatabase, env: RecoveryEnv, user: User, url: string, token: string, now = Date.now()) { await application(control, env, sendBetaEmail, now).queue(user.id, url, token); }
export async function deliverAccountRecovery(control: SqlDatabase, env: RecoveryEnv, send: RecoverySender = sendBetaEmail, now = Date.now()) { return application(control, env, send, now).deliver(); }
export async function recordAccountRecoveryCompleted(control: SqlDatabase, userId: string, now = Date.now()) {
    // Completion never needs cryptography or an email provider.
    return createAccountRecoveryApplication({ repository: repository(control), clock: { now: () => now }, identifiers: { next: () => crypto.randomUUID() }, productOrigins: {}, cryptography: { recoveryId: () => { throw new Error('RECOVERY_NOT_REQUESTED'); }, sealLink: () => { throw new Error('RECOVERY_NOT_REQUESTED'); }, openLink: () => { throw new Error('RECOVERY_NOT_REQUESTED'); } }, send: async () => { throw new Error('RECOVERY_NOT_REQUESTED'); } }).complete(userId);
}
