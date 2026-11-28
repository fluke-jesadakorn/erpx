import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeWorkflowWrite} from './native-workflow-write';
export function financeBusinessContractEffects(db: SqlDatabase) {
    return { contractDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) {
            return nativeWorkflowWrite('wf_native_finance_business_contract_effects_5', [value, key, id, op, userId, assignmentId, expectedVersion, value2, now]);
        }, recognitionDecisionEvidence(value: string, key: string, id: string, op: string, userId: string, assignmentId: string, expectedVersion: number, value2: string, now: number) {
            return nativeWorkflowWrite('wf_native_finance_business_contract_residual', [value, key, id, op, userId, assignmentId, expectedVersion, value2, now]);
        } };
}
