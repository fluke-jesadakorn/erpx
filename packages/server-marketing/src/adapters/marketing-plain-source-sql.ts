import "server-only";

/** Owner-local fixed plain-source projection shared by the legacy candidate
 * reader and the package settlement readers. A caller selects an admitted
 * variant and never supplies raw SQL or a relation name; each variant emits its
 * original statement byte for byte, so no duplicate projection literal survives
 * in the owner. */
export function marketingPlainSourceSql(
  source: 'candidate' | 'engagements' | 'costs' | 'touches' | 'campaigns',
): string {
  const table = source === 'candidate' ? 'marketing_lead_candidates'
    : source === 'engagements' ? 'marketing_engagement_events'
      : source === 'costs' ? 'marketing_campaign_costs'
        : source === 'touches' ? 'marketing_touches' : 'marketing_campaigns';
  return `SELECT * FROM ${table}`;
}
