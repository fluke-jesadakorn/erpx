import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import {nativeWorkflowWrite} from './native-workflow-write';
export function systemPayrollPreviewEvents(database: SqlDatabase) {
    return { stageDrainPayrollScheduledPreviewsCompleted3(value: string, commandId: string, id: string, value2: number, timestamp: number) {
            return nativeWorkflowWrite('wf_native_system_payroll_preview_events_24', [value, commandId, id, value2, timestamp]).run();
        },
        stageDrainPayrollScheduledPreviewsCompleted4(value: string, commandId: string, id: string, value2: number, value3: string, timestamp: number) {
            return nativeWorkflowWrite('wf_native_system_payroll_preview_events_25', [value, commandId, id, value2, value3, timestamp]).run();
        } };
}
