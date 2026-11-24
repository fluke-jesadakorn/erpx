import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeWorkflowReadAll } from './native-workflow-read';
export function calendarIndexWorkflowFacts(db: SqlDatabase) {
    return { read4: (facts: {
            value1: string;
            value2: string;
        }) => nativeWorkflowReadAll<{
                 record_id: string;
                 action: string;
                 actor: string;
                 at: number;
             }>('wf_native_calendar_index_facts', [facts.value1, facts.value1, facts.value2], facts.value1, db) };
}
