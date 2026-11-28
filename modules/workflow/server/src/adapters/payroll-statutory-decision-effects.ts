import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeWorkflowWrite} from './native-workflow-write';
export function payrollStatutoryDecisionEffects(database:()=>SqlDatabase){return {decisionEvidence(value: string, key: string, id: string, value2: string, userId: string, assignmentId: string, version: number, value3: string, now: number) { return nativeWorkflowWrite('wf_native_payroll_statutory_decision_effects_17', [value, key, id, value2, userId, assignmentId, version, value3, now]); }};}
