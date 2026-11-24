import 'server-only';
/** Canonical byte-identical SELECT-only owner fragments shared across People Payroll
 * adapters (G4 read-tail r22): jsonb_agg projections, snapshot projection tails,
 * workforce scope variants with different extra columns and id/version/status
 * guards not covered by r21. Each constant owns one literal copied verbatim from
 * its former inline copies; consumers import it and keep no local copy. All are
 * pure reads: no INSERT, UPDATE, DELETE, LOCK TABLE, FOR UPDATE/SHARE or advisory lock. */
/** jsonb_agg engagement projection used by worker reference witnesses */
export const PP_RD22_ENGAGEMENT_JSONB_AGG_SQL =
  'SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM hr_engagements e WHERE e.person_id=p.id AND e.company_id=p.company_id';
/** leave_requests snapshot projection tail shared by owner projection sources */
export const PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL =
  'SELECT "id","number","employee_id","company_id","branch_id","start_date","days","handover_note","state","version","created_by","prepared_by","decided_by","decision_note","last_command_id","created_at","updated_at","department_id","person_id","engagement_id","leave_type_id","end_date","start_segment","end_segment","policy_version_id","calculated_units","calculation_snapshot","cancellation_state","evidence_policy_snapshot","evidence_due_at","evidence_version","submitted_at" FROM leave_requests';
/** workforce_actor_scope prefix for scope variants with different extra columns */
export const PP_RD22_WORKFORCE_SCOPE_SQL =
  'SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ';
/** active person id/company/version/status guard (not an r21 builder) */
export const PP_RD22_ACTIVE_PERSON_VERSION_GUARD =
  "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND version=? AND status='active' AND merged_into_id IS NULL)";
/** engagement id/company/version guard without status (not an r21 builder) */
export const PP_RD22_ENGAGEMENT_VERSION_GUARD =
  'EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)';
