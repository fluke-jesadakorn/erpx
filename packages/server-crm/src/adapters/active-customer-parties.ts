import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import "server-only";

/** One CRM-owned fixed-SQL read of the current active Finance customers, shared
 * by the identity and lead applications so both callers execute identical query
 * text instead of two independently maintained literals. */
export const activeCustomerPartiesSql = `SELECT id,name FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name`;
