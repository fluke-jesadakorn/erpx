import { z } from 'zod';
export const recoveryInput = z.object({
    holdingSlug: z.string().optional(),
    companyId: z.string().min(1),
    reason: z.string().trim().min(10).max(1000),
});
export const emergencyRevocationInput = recoveryInput;
export type EmergencyActor = {
    userId: string;
    sessionId: string;
};
export type RecoveryScope = {
    companyId: string;
    holdingSlug: string | null;
    scopeLabel: string;
};
export type RecoveryInput = z.infer<typeof recoveryInput>;
export const validRecoveryKey = (key: string) => /^[a-zA-Z0-9:_.-]{8,180}$/.test(key);
