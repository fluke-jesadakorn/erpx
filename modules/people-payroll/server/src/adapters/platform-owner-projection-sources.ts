import {ppRd42FourColOwnerProjection} from './pp-read-tail-r42-sources';
import { PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL } from './pp-read-tail-r22-sources';
import { PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL } from './pp-read-tail-r20-sources';
import {ppRd45CompRevisionProjection} from './pp-read-tail-r45-sources';
import "server-only";
import {PP_RD18_HR_PEOPLE_PROJ_SQL} from './pp-read-tail-r18-sources';
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  employees: ppRd42FourColOwnerProjection('"id","user_id","company_id","branch_id","display_name","status","created_at","department_id","person_id"','employees'),
  hr_compensation_revisions: ppRd45CompRevisionProjection("\"id\",\"engagement_id\",\"effective_from\",\"effective_to\",\"pay_frequency\",\"base_amount_minor\",\"hourly_rate_minor\",\"expected_hours\",\"currency\",\"components_json\",\"status\",\"version\",\"approved_by\",\"created_by\",\"created_at\",\"updated_at\",\"daily_rate_minor\",\"standard_daily_hours\""),
  hr_engagements: PP_RD20_HR_ENGAGEMENTS_COMPACT_SQL,
  hr_pay_groups: ppRd42FourColOwnerProjection('"id","company_id","name","country_code","currency","frequency","timezone","calendar_json","status","created_by","created_at","updated_at","overtime_multiplier_bps","proration_method","schedule_enabled","pay_delay_days","schedule_anchor_date","schedule_version"','hr_pay_groups'),
  hr_people: PP_RD18_HR_PEOPLE_PROJ_SQL,
  leave_day_segments: ppRd42FourColOwnerProjection('"id","request_id","company_id","leave_date","segment","units","source"','leave_day_segments'),
  leave_policy_assignments: ppRd42FourColOwnerProjection('"id","company_id","engagement_id","policy_version_id","holiday_calendar_id","effective_from","effective_to","created_by","created_at","version"','leave_policy_assignments'),
  leave_policy_rules: ppRd42FourColOwnerProjection('"policy_version_id","leave_type_id","grant_kind","grant_units","accrual_units","accrual_period","carryover_limit","employee_eligible","contractor_eligible","evidence_min_units","evidence_purpose","evidence_min_count","evidence_due_stage","evidence_grace_days","evidence_note","evidence_privacy"','leave_policy_rules'),
  leave_policy_versions: ppRd42FourColOwnerProjection('"id","company_id","policy_key","version","name","status","effective_from","effective_to","timezone","workweek_json","allow_negative","created_by","created_at"','leave_policy_versions'),
  leave_requests: PP_RD22_LEAVE_REQUESTS_PROJECTION_SQL,
  leave_types: ppRd42FourColOwnerProjection('"id","company_id","code","name","description","tracked","evidence_after_units","evidence_note","private_evidence","active","effective_from","effective_to","created_by","created_at","updated_at"','leave_types'),
  people_applied_packages: ppRd42FourColOwnerProjection('"id","company_id","person_id","engagement_id","position_id","package_id","package_revision","definition","source_kind","scope","case_id","operation_id","authorizing_assignment_id","valid_from","valid_until","ended_at","created_at"','people_applied_packages'),
  people_package_revisions: ppRd42FourColOwnerProjection('"company_id","package_id","revision","definition","created_by","created_at"','people_package_revisions'),
  people_position_packages: ppRd42FourColOwnerProjection('"company_id","position_id","package_id","revision"','people_position_packages'),
  people_roster_assignments: ppRd42FourColOwnerProjection('"id","shift_id","person_id","engagement_id","state","version"','people_roster_assignments'),
  people_roster_shifts: ppRd42FourColOwnerProjection('"id","week_id","template_id","name","local_date","start_time","end_time","starts_at","ends_at","capacity","state","version"','people_roster_shifts'),
  people_roster_weeks: ppRd42FourColOwnerProjection('"id","company_id","branch_id","department_id","week_start","timezone","state","version","created_by","published_by","created_at","updated_at"','people_roster_weeks'),
} as const);
