/** Canonical owner-local fixed SQL for the Marketing campaign existence guard.
 * The public submission unit and the campaign command assertion share this one
 * predicate, so no duplicate SQL literal remains. Callers supply only values. */
export const marketingCampaignExistsSql = "SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?";
