export const financeBankPosCashAvailableSql="NOT EXISTS(SELECT 1 FROM pos_settlements s WHERE s.company_id=j.company_id AND s.journal_id=j.id)";
