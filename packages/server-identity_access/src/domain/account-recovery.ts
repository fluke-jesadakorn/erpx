export type RecoveryDeliveryJob = {
    id: string;
    user_id: string;
    url_ciphertext: string;
    attempts: number;
    expires_at: number;
};
export function recoveryDeliveryCancellation(account: {
    email: string;
} | null, expiresAt: number, now: number): string | null { return !account ? "ACCOUNT_UNAVAILABLE" : expiresAt <= now ? "RECOVERY_EXPIRED" : null; }
export function recoveryDeliveryFailure(error: unknown, attempts: number, now: number) {
    const code = error instanceof Error && error.message === "EMAIL_NOT_CONFIGURED" ? error.message : "EMAIL_DELIVERY_FAILED";
    return { state: attempts >= 6 ? "failed" as const : "queued" as const, terminal: attempts >= 6, code, nextAttemptAt: now + Math.min(3600000, 30000 * 2 ** attempts) };
}
export function recoveryEmailProduct(link: string, origins: {
    pms?: string;
    pos?: string;
}): string {
    const host = new URL(link).host;
    return origins.pms && host === new URL(origins.pms).host ? "PMS Front" : origins.pos && host === new URL(origins.pos).host ? "POS Front" : "ERPX";
}
