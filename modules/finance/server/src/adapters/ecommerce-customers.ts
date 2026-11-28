import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type {Row} from '@erpx/commerce-server/public/commerce';
import { finPartyIdNameByCompanySql } from './finance-read-tail-r57-sql';

export async function ecommerceCustomerCandidates(db: SqlDatabase, companyId: string) {
  return (await db.prepare(finPartyIdNameByCompanySql(" AND kind='customer' AND status='active' ORDER BY name")).bind(companyId).all<Row>()).results;
}
