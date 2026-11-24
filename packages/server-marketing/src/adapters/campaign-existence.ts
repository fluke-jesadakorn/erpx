import { marketingOwnerPointReadSql } from "./owner-point-read-sql";
/** Canonical owner-local fixed SQL for the Marketing campaign existence guard.
 * The public submission unit and the campaign command assertion share this one
 * predicate, so no duplicate SQL literal remains. Callers supply only values. */
export const marketingCampaignExistsSql = marketingOwnerPointReadSql("1", "marketing_campaigns");
