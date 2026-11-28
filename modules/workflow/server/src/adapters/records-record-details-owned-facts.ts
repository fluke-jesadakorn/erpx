import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import {type Row} from '@erpx/commerce-server/public/commerce';
import { nativeWorkflowReadAll } from './native-workflow-read';
export function RecordDetailsWorkflowOwnedFacts(database: SqlDatabase) {
    return {
        async readCommerceMasterDetailHistory(id: string, companyId: string) {
            const results = await nativeWorkflowReadAll<Row>('wf_native_record_details_history', [id], companyId, database);
            return {results};
        },
    };
}
