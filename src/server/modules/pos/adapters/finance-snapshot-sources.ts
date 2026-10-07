import "server-only";
/** Closed POS source projections retain original aliases, scopes, and supplied transaction snapshot. */
export function financeSnapshotSources(){return {
  "completedSales": "FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed'",
  "missingPosting": "NOT EXISTS(SELECT 1 FROM pos_postings pp WHERE pp.company_id=s.company_id AND pp.source_key='sale:'||s.id)",
  "successfulTransfers": "FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id\n    WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND p.method='transfer' AND p.state='succeeded'",
  "inventoryAccounts": "SELECT DISTINCT s.accounts_json::jsonb->>'inventory' FROM pos_sales s WHERE s.company_id=j.company_id AND s.branch_id=?",
  "dailySettings": "JOIN pos_settings c ON c.company_id=j.company_id"
} as const;}
