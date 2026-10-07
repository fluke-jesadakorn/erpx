export function createPasswordResetCompleted(ports: {
    record(userId: string): Promise<void>;
    revokeSessions(userId: string): Promise<void>;
    auditFailure(userId: string): void;
}) {
    return async (userId: string) => {
        try {
            await ports.record(userId);
        } catch {
            await ports.revokeSessions(userId);
            ports.auditFailure(userId);
        }
    };
}
