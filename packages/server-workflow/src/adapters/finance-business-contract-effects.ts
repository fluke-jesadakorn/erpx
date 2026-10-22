import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function financeBusinessContractEffects(db: SqlDatabase) {
    return { contractDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) {
            return db
                .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(value, key, id, op, userId, assignmentId, expectedVersion, value2, now);
        }, recognitionDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) {
            return db
                .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(value, key, id, op, userId, assignmentId, expectedVersion, value2, now);
        } };
}
