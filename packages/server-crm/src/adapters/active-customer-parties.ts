import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import {crmSingleKeySourceSql} from "./read-tail-r37-sql";
import "server-only";

/** One CRM-owned fixed-SQL read of the current active Finance customers, shared
 * by the identity and lead applications so both callers execute identical query
 * text instead of two independently maintained literals. */
export const activeCustomerPartiesSql = crmSingleKeySourceSql('id,name',financeProjectionSources.finance_parties,'finance_parties'," AND kind='customer' AND status='active' ORDER BY name");
