import type { Clock, IdentifierSource } from "../../shared";
import { recoveryDeliveryCancellation, recoveryDeliveryFailure, recoveryEmailProduct, type RecoveryDeliveryJob } from "../domain/account-recovery";
export type RecoveryQueueRequest = {
    id: string;
    userId: string;
    verificationIdentifier: string;
    urlCiphertext: string;
    now: number;
    expiresAt: number;
    eventId: string;
};
export interface AccountRecoveryRepository {
    queue(request: RecoveryQueueRequest): Promise<void>;
    recoverInterrupted(now: number): Promise<void>;
    claim(lease: string, now: number): Promise<RecoveryDeliveryJob | null>;
    account(userId: string): Promise<{
        email: string;
    } | null>;
    cancel(job: RecoveryDeliveryJob, lease: string, reason: string, now: number, eventId: string): Promise<void>;
    sent(job: RecoveryDeliveryJob, lease: string, providerId: string | null, now: number, eventId: string): Promise<void>;
    failed(job: RecoveryDeliveryJob, lease: string, failure: ReturnType<typeof recoveryDeliveryFailure>, now: number, eventId: string): Promise<void>;
    complete(userId: string, now: number, eventId: string): Promise<void>;
}
export interface AccountRecoveryPorts {
    repository: AccountRecoveryRepository;
    clock: Clock;
    identifiers: IdentifierSource;
    cryptography: {
        recoveryId(token: string): string;
        sealLink(url: string): string;
        openLink(ciphertext: string): string;
    };
    productOrigins: {
        pms?: string;
        pos?: string;
    };
    send(to: string, subject: string, url: string, key: string): Promise<string | null | void>;
}
export function createAccountRecoveryApplication(ports: AccountRecoveryPorts) {
    return {
        async queue(userId: string, url: string, token: string): Promise<void> {
            const now = ports.clock.now();
            await ports.repository.queue({ id: ports.cryptography.recoveryId(token), userId, verificationIdentifier: `reset-password:${token}`, urlCiphertext: ports.cryptography.sealLink(url), now, expiresAt: now + 3600000, eventId: ports.identifiers.next() });
        },
        async deliver(): Promise<boolean> {
            const now = ports.clock.now();
            await ports.repository.recoverInterrupted(now);
            const lease = ports.identifiers.next(), job = await ports.repository.claim(lease, now);
            if (!job)
                return false;
            const account = await ports.repository.account(job.user_id);
            const cancellation = recoveryDeliveryCancellation(account, job.expires_at, now);
            if (cancellation) {
                await ports.repository.cancel(job, lease, cancellation, now, ports.identifiers.next());
                return true;
            }
            if (!account)
                throw new Error("RECOVERY_ACCOUNT_REQUIRED");
            try {
                const link = ports.cryptography.openLink(job.url_ciphertext);
                const product = recoveryEmailProduct(link, ports.productOrigins);
                const providerId = await ports.send(account.email, `Reset your ${product} password`, link, job.id);
                await ports.repository.sent(job, lease, typeof providerId === "string" ? providerId : null, now, ports.identifiers.next());
            }
            catch (error) {
                await ports.repository.failed(job, lease, recoveryDeliveryFailure(error, job.attempts, now), now, ports.identifiers.next());
            }
            return true;
        },
        async complete(userId: string): Promise<void> { await ports.repository.complete(userId, ports.clock.now(), ports.identifiers.next()); },
    };
}
