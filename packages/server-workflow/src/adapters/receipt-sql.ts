export const receiptRecordIdSql = "SELECT record_id FROM command_receipts WHERE command_id=?";
export const exportReceiptResultSql = "SELECT result_json FROM command_receipts WHERE command_id=? AND operation=?";
