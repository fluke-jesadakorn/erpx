import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
export function projectactualsSources(db: SqlDatabase) {
    return { async commandReceipt(key: string) { return db.prepare("SELECT 1").bind().first<{
            operation: string;
            record_id: string;
        }>(); },
        async costCommandReceipt(key: string) { return db.prepare("SELECT 1").bind().first<{
            operation: string;
            record_id: string;
        }>(); } };
}
