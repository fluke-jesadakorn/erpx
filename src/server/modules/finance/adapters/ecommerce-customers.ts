import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { Row } from "../../commerce/public/commerce";

export async function ecommerceCustomerCandidates(db: SqlDatabase, companyId: string) {
  return (await db.prepare("SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name").bind(companyId).all<Row>()).results;
}
