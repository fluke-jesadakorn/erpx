import 'server-only';
/** Owner-local fixed projection for the Marketing workspace candidate list and
 * detail readers. Both callers supply their own Company, authority, branch and
 * department filters and binds; one canonical row source keeps the returned
 * candidate plus form-name shape from drifting between them. */
export function marketingCandidateFormRows(): string {
  return `SELECT c.*,a.name form_name FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id`;
}
