/** One byte-identical head shared by the two UNION ALL branches of the beta
 * AI-usage read. Only a closed projection token is admitted; the head text stays
 * here so neither branch can drift and no request text is interpolated. */
export type UsageHeadProjection =
  | 'count(*) requests,sum(input_tokens) inputTokens,sum(output_tokens) outputTokens'
  | 'input_tokens,output_tokens';
export const usageHeadSql = (projection: UsageHeadProjection) => `SELECT ${projection} FROM (`;
