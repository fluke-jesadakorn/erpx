import 'server-only';

/** Owner-local fixed evidence row projections shared by the executive evidence
 * search and the workspace detail reader. The caller supplies the candidate
 * source expression and its own binds; one canonical projection keeps the two
 * readers from drifting on the returned row shape. */
export function marketingEvidenceCandidateRows(candidateSource: string): string {
  return `SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name
      FROM ${candidateSource} c JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id`;
}

export function marketingEvidenceEventRows(candidateSource: string): string {
  return `SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name
      FROM marketing_engagement_events e JOIN ${candidateSource} c ON c.id=e.candidate_id JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id`;
}
