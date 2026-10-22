import "server-only";
/** Closed POS source projections retain original aliases, scopes, and supplied transaction snapshot. */
export function financeSnapshotSources(){return {
  "completedSales": "FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed'",
  "missingPosting": "NOT EXISTS(SELECT 1 FROM pos_postings pp WHERE pp.company_id=s.company_id AND pp.source_key='sale:'||s.id)",
  "successfulTransfers": "FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id\n    WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND p.method='transfer' AND p.state='succeeded'",
  "inventoryAccounts": "SELECT DISTINCT s.accounts_json::jsonb->>'inventory' FROM pos_sales s WHERE s.company_id=j.company_id AND s.branch_id=?",
  "linkedProperty": "s.register_id IN (SELECT id FROM pos_registers WHERE property_id=?)",
  "linkedJournal": "EXISTS(SELECT 1 FROM pos_registers r WHERE r.property_id=? AND (EXISTS(SELECT 1 FROM pos_sales s WHERE s.register_id=r.id AND s.company_id=j.company_id AND (s.id=j.source_id OR EXISTS(SELECT 1 FROM pos_lines li WHERE li.sale_id=s.id AND li.fulfillment_id=j.source_id) OR EXISTS(SELECT 1 FROM pos_refunds rf WHERE rf.sale_id=s.id AND rf.id=j.source_id))) OR EXISTS(SELECT 1 FROM pos_shifts sh WHERE sh.register_id=r.id AND sh.id=j.source_id AND sh.company_id=j.company_id)))",
  "dailySettings": "JOIN pos_settings c ON c.company_id=j.company_id"
} as const;}
