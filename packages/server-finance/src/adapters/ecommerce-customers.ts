import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type {Row} from '@erpx/server-commerce/public/commerce';
import { finPartyIdNameByCompanySql } from './finance-read-tail-r57-sql';

export async function ecommerceCustomerCandidates(db: SqlDatabase, companyId: string) {
  return (await db.prepare(finPartyIdNameByCompanySql(" AND kind='customer' AND status='active' ORDER BY name")).bind(companyId).all<Row>()).results;
}
