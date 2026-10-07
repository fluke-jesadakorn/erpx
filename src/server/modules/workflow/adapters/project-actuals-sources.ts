import 'server-only';
import type { SqlDatabase } from '../../runtime/public/runtime/types';
export function projectactualsSources(db: SqlDatabase) {
    return { async commandReceipt(key: string) { return db.prepare('SELECT operation,record_id FROM command_receipts WHERE command_id=?').bind(key).first<{
            operation: string;
            record_id: string;
        }>(); },
        async costCommandReceipt(key: string) { return db.prepare('SELECT operation,record_id FROM command_receipts WHERE command_id=?').bind(key).first<{
            operation: string;
            record_id: string;
        }>(); } };
}
