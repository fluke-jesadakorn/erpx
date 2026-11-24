import 'server-only';

/** Owner-local fixed evidence row projections shared by the executive evidence
 * search and the workspace detail reader. The caller supplies the candidate
 * source expression and its own binds; one closed variant builder keeps the two
 * readers from drifting on the returned row shape or the shared asset join. */
type EvidenceVariant = 'candidate' | 'event';

const evidenceProjection: Record<EvidenceVariant, string> = {
  candidate:
    'c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name',
  event:
    'e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name',
};

const evidenceFrom: Record<EvidenceVariant, (candidateSource: string) => string> = {
  candidate: (candidateSource) => `${candidateSource} c`,
  event: (candidateSource) =>
    `marketing_engagement_events e JOIN ${candidateSource} c ON c.id=e.candidate_id`,
};

const assetJoin = ' JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id';

function evidenceRows(variant: EvidenceVariant, candidateSource: string): string {
  return `SELECT ${evidenceProjection[variant]}
      FROM ${evidenceFrom[variant](candidateSource)}${assetJoin}`;
}

export function marketingEvidenceCandidateRows(candidateSource: string): string {
  return evidenceRows('candidate', candidateSource);
}

export function marketingEvidenceEventRows(candidateSource: string): string {
  return evidenceRows('event', candidateSource);
}
