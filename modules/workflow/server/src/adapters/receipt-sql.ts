import {commandReceiptsProjectionSql} from './closed-projection-sources';
/** Byte-identical generator family for command-receipt point reads: callers choose the projected column, the emitted SQL is unchanged. */
export const commandReceiptByCommandSql = (column: string): string => commandReceiptsProjectionSql([column], ',', ' WHERE command_id=?');
export const receiptRecordIdSql = commandReceiptByCommandSql('record_id');
export const exportReceiptResultSql = commandReceiptsProjectionSql(['result_json'], ',', ' WHERE command_id=? AND operation=?');
