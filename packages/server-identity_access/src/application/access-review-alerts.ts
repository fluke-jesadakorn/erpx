export type AccessReviewAlertResult = {
    dueSoon: number;
    overdue: number;
};
export type AccessReviewAlertPhase = 'due-soon' | 'overdue';
export function createAccessReviewAlerts(ports: {
    emit(phase: AccessReviewAlertPhase, now: number, horizon: number, message: string): Promise<number>;
    read(companyId: string, userId: string, campaignId: string, now: number): Promise<boolean>;
}) { return { escalate: async (now: number, warningWindowMs = 7 * 86400000): Promise<AccessReviewAlertResult> => { const dueSoon = await ports.emit('due-soon', now, now + warningWindowMs, 'Access certification is due within 7 days; review pending assignments or coordinate an independent reviewer'); const overdue = await ports.emit('overdue', now, now, 'Access certification is overdue; complete the remaining independent decisions'); return { dueSoon, overdue }; }, canRead: ports.read }; }
