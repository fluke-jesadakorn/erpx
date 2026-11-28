//! Native people-payroll write statements, owned by this crate.
//! Generated from the migrated native statement contracts; every kind here is a
//! real Postgres command executed by the owner adapter inside the composition
//! unit of work. There is no TypeScript SQL mirror and no fallback path.
//! Bindings use positional `$n` placeholders; the domain allow-list admits only
//! the kinds enumerated below.

/// A supported write-statement kind with its retained bind arity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeStatement {
    pub kind: &'static str,
    pub sql: &'static str,
    pub bind_count: usize,
}

const STATEMENTS: &[NativeStatement] = &[
    NativeStatement {
        kind: "leave_ss_stmt_1",
        sql: r#"INSERT INTO leave_requests(id,number,employee_id,person_id,engagement_id,company_id,branch_id,department_id,start_date,days,handover_note,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)"#,
        bind_count: 14,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_2",
        sql: r#"UPDATE leave_requests SET start_date=$1,days=$2,handover_note=$3,leave_type_id=$4,end_date=$5,start_segment=$6,end_segment=$7,policy_version_id=$8,calculated_units=$9,calculation_snapshot=$10::jsonb,
    evidence_policy_snapshot=$11::jsonb,evidence_due_at=$12,submitted_at=CASE WHEN $13='pending_approval' THEN $14 ELSE submitted_at END,
    state=$15,version=$16,prepared_by=$17,decided_by=NULL,decision_note=NULL,last_command_id=$18,updated_at=$19 WHERE id=$20"#,
        bind_count: 20,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_3",
        sql: r#"DELETE FROM leave_day_segments WHERE request_id=$1"#,
        bind_count: 1,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_4",
        sql: r#"INSERT INTO leave_day_segments(id,request_id,company_id,leave_date,segment,units,source) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_5",
        sql: r#"INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,request_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at) VALUES($1,$2,$3,$4,$5,'reservation',$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_6",
        sql: r#"INSERT INTO leave_types(id,company_id,code,name,description,tracked,evidence_after_units,evidence_note,effective_from,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_7",
        sql: r#"INSERT INTO leave_holiday_calendars(id,company_id,name,timezone,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6)"#,
        bind_count: 6,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_8",
        sql: r#"INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,effective_to,timezone,workweek_json,allow_negative,created_by,created_at) VALUES($1,$2,$3,$4,$5,'active',$6,$7,$8,$9::jsonb,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_9",
        sql: r#"INSERT INTO leave_policy_rules(
      policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units,accrual_period,carryover_limit,employee_eligible,contractor_eligible,
      evidence_min_units,evidence_purpose,evidence_min_count,evidence_due_stage,evidence_grace_days,evidence_note,evidence_privacy
    ) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,'requester_approvers_hr' WHERE EXISTS(SELECT 1 FROM leave_types WHERE id=$16 AND company_id=$17)"#,
        bind_count: 17,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_10",
        sql: r#"INSERT INTO leave_holidays(id,calendar_id,holiday_date,name) SELECT $1,$2,$3,$4 WHERE EXISTS(SELECT 1 FROM leave_holiday_calendars WHERE id=$5 AND company_id=$6)"#,
        bind_count: 6,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_11",
        sql: r#"UPDATE leave_policy_assignments SET effective_to=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_12",
        sql: r#"INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,holiday_calendar_id,effective_from,effective_to,created_by,created_at) SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9 WHERE EXISTS(SELECT 1 FROM hr_engagements WHERE id=$10 AND company_id=$11)"#,
        bind_count: 11,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_13",
        sql: r#"UPDATE leave_policy_assignments SET effective_to=$1,version=version+1 WHERE id=$2 AND company_id=$3"#,
        bind_count: 3,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_14",
        sql: r#"INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,note,created_by,created_at) VALUES($1,$2,$3,$4,'adjustment',$5,$6,$7,$8,$9,$10) ON CONFLICT(company_id,command_key,kind,leave_type_id) DO NOTHING"#,
        bind_count: 10,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "leave_ss_stmt_15",
        sql: r#"INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,kind,units,effective_date,command_key,note,created_by,created_at)
    SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,'system:leave-accrual',$10 WHERE TRUE
    ON CONFLICT(company_id,command_key,kind,leave_type_id) DO NOTHING"#,
        bind_count: 10,
    }, // native-leave-self-service-sql
    NativeStatement {
        kind: "people_eval_stmt_1",
        sql: r#"UPDATE hr_evaluation_definitions SET payload_json=$1,version=version+1,updated_by=$2,reason=$3,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_2",
        sql: r#"INSERT INTO hr_evaluation_definitions(id,company_id,kind,payload_json,created_by,updated_by,reason,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_3",
        sql: r#"INSERT INTO hr_evaluation_definitions(id,company_id,kind,payload_json,created_by,updated_by,reason,created_at,updated_at) VALUES($1,$2,'benchmark',$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_4",
        sql: r#"INSERT INTO hr_evaluation_assessments(id,company_id,engagement_id,cycle_id,role_id,role_version,period_start,period_end,content_json,created_by,reason,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        bind_count: 13,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_5",
        sql: r#"UPDATE hr_evaluation_assessments SET period_start=$1,period_end=$2,content_json=$3,reviewed_by=$4,updated_by=$5,reason=$6,version=version+1,updated_at=$7 WHERE id=$8"#,
        bind_count: 8,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_6",
        sql: r#"UPDATE hr_evaluation_assessments SET status=$1,updated_by=$2,confirmed_by=$3,reviewed_by=CASE WHEN $4='draft' THEN NULL ELSE reviewed_by END,reason=$5,version=version+1,updated_at=$6 WHERE id=$7"#,
        bind_count: 7,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_7",
        sql: r#"INSERT INTO hr_evaluation_evidence(id,company_id,assessment_id,source_type,source_id,description,observed_on,source_json,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_8",
        sql: r#"UPDATE hr_evaluation_assessments SET version=version+1,reviewed_by=$1,updated_by=$2,reason='Evidence added',updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_9",
        sql: r#"INSERT INTO hr_evaluation_pay_basis(engagement_id,company_id,version,effective_from,payload_json,confirmed_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_10",
        sql: r#"INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,daily_rate_minor,standard_daily_hours,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'draft',$13,$14,$15)"#,
        bind_count: 15,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_11",
        sql: r#"INSERT INTO hr_evaluation_compensation_drafts(id,company_id,assessment_id,assessment_version,compensation_id,previous_id,previous_version,engagement_version,policy_id,policy_version,basis_version,reason,created_by,created_at,updated_at,previous_json,proposed_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)"#,
        bind_count: 17,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_12",
        sql: r#"UPDATE hr_compensation_revisions SET effective_to=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_13",
        sql: r#"UPDATE hr_compensation_revisions SET status='approved',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_14",
        sql: r#"UPDATE hr_evaluation_compensation_drafts SET status='approved',approved_by=$1,review_reason=$2,version=version+1,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_15",
        sql: r#"UPDATE hr_compensation_revisions SET status='superseded',version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_eval_stmt_16",
        sql: r#"UPDATE hr_evaluation_compensation_drafts SET status='cancelled',review_reason=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-eval-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_1",
        sql: r#"UPDATE people_lifecycle_cases SET status='cancelled',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3"#,
        bind_count: 3,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_2",
        sql: r#"UPDATE people_preparation_tasks SET status='cancelled',version=version+1,updated_at=$1 WHERE company_id=$2 AND case_id=$3 AND status='pending'"#,
        bind_count: 3,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_3",
        sql: r#"INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,package_id,package_revision,definition,created_by,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(id) DO UPDATE SET status=excluded.status,version=excluded.version,effective_at=excluded.effective_at,package_id=excluded.package_id,package_revision=excluded.package_revision,definition=excluded.definition,updated_at=excluded.updated_at"#,
        bind_count: 13,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_4",
        sql: r#"UPDATE people_lifecycle_cases SET review_snapshot=$1,authorizing_assignment_id=$2,workflow=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_5",
        sql: r#"INSERT INTO people_access_cutoffs(company_id,person_id,case_id,effective_at) VALUES($1,$2,$3,$4)"#,
        bind_count: 4,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_6",
        sql: r#"UPDATE people_lifecycle_cases SET departure_reason=$1 WHERE id=$2 AND company_id=$3"#,
        bind_count: 3,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_7",
        sql: r#"INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,assignee_id,updated_at,privileged,due_date,responsibility,clearance_type) VALUES($1,$2,$3,'lifecycle',$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_8",
        sql: r#"INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,assignee_id,updated_at,privileged) VALUES($1,$2,$3,'lifecycle',$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING"#,
        bind_count: 7,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_9",
        sql: r#"UPDATE people_lifecycle_cases SET status='cancelled',version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_10",
        sql: r#"UPDATE people_access_cutoffs SET cancelled_at=$1 WHERE case_id=$2"#,
        bind_count: 2,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_11",
        sql: r#"UPDATE people_preparation_tasks SET status='cancelled',version=version+1 WHERE case_id=$1 AND status='pending'"#,
        bind_count: 1,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_12",
        sql: r#"INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,position_id,level_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'onboarding',$10,$11,$12,$13,$14,$15)"#,
        bind_count: 15,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_13",
        sql: r#"UPDATE hr_people SET status='active',version=version+1,updated_at=$1 WHERE company_id=$2 AND id=$3"#,
        bind_count: 3,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_14",
        sql: r#"INSERT INTO people_lifecycle_settings VALUES($1,$2,$3) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,definition=excluded.definition"#,
        bind_count: 3,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_15",
        sql: r#"UPDATE people_preparation_tasks SET assignee_id=$1,version=version+1 WHERE company_id=$2 AND assignee_id IS NULL AND status='pending' AND clearance_type IS NULL"#,
        bind_count: 2,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_16",
        sql: r#"INSERT INTO people_access_packages(company_id,id,version,published_revision,definition) VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,published_revision=excluded.published_revision,definition=excluded.definition"#,
        bind_count: 5,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_17",
        sql: r#"INSERT INTO people_package_revisions VALUES($1,$2,$3,$4,$5,$6)"#,
        bind_count: 6,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_18",
        sql: r#"UPDATE people_position_packages SET revision=$3 WHERE company_id=$1 AND package_id=$2"#,
        bind_count: 3,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_19",
        sql: r#"INSERT INTO people_position_packages VALUES($1,$2,$3,$4) ON CONFLICT(company_id,position_id) DO UPDATE SET package_id=excluded.package_id,revision=excluded.revision"#,
        bind_count: 4,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_20",
        sql: r#"UPDATE people_preparation_tasks SET status='completed',version=version+1,updated_at=$3 WHERE company_id=$1 AND position_id=$2 AND kind='position'"#,
        bind_count: 3,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_21",
        sql: r#"INSERT INTO people_lifecycle_grants(id,company_id,case_id,user_id,grant_index,definition,review_id) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_22",
        sql: r#"UPDATE people_lifecycle_cases SET version=version+1,updated_at=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_23",
        sql: r#"UPDATE people_lifecycle_cases SET status='blocked',last_error='ACCESS_REVIEW_REJECTED',version=version+1,updated_at=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_24",
        sql: r#"UPDATE people_lifecycle_grants SET assignment_id=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_lifecycle_stmt_25",
        sql: r#"UPDATE people_lifecycle_cases SET status='scheduled',version=version+1,updated_at=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-people-lifecycle-sql
    NativeStatement {
        kind: "people_payroll_stmt_1",
        sql: r#"UPDATE hr_payroll_cost_assignments SET evidence_status='verified',source='hr_attestation',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4"#,
        bind_count: 4,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_2",
        sql: r#"INSERT INTO hr_payroll_cost_assignment_verifications(id,company_id,assignment_id,verified_by,evidence_ref,previous_version,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_3",
        sql: r#"UPDATE hr_time_entries t SET cost_assignment_id=a.id,cost_branch_id=a.branch_id,cost_department_id=a.department_id,version=t.version+1,updated_at=$1
      FROM hr_payroll_cost_assignments a WHERE a.id=$2 AND a.company_id=$3 AND t.engagement_id=a.engagement_id AND t.status='approved'
      AND t.work_date>=a.effective_from AND (a.effective_to IS NULL OR t.work_date<=a.effective_to)
      AND t.cost_assignment_id IS NULL"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_4",
        sql: r#"INSERT INTO hr_command_receipts(command_id,operation,request_hash,result_json,created_at) VALUES($1,$2,$3,$4,$5)"#,
        bind_count: 5,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_5",
        sql: r#"INSERT INTO hr_people(id,user_id,company_id,legal_name,preferred_name,email,phone,country_code,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        bind_count: 11,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_6",
        sql: r#"INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,end_date,manager_person_id,job_title,currency,pay_group_id,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)"#,
        bind_count: 17,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_7",
        sql: r#"INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,daily_rate_minor,standard_daily_hours,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'draft',$13,$14,$15)"#,
        bind_count: 15,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_8",
        sql: r#"UPDATE hr_engagements SET pay_group_id=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_9",
        sql: r#"UPDATE hr_compensation_revisions SET status=$1,approved_by=$2,version=version+1,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_10",
        sql: r#"UPDATE hr_engagements SET status=$1,end_date=CASE WHEN $2='ended' THEN $3 ELSE end_date END,version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_11",
        sql: r#"UPDATE employees SET status=CASE WHEN EXISTS(SELECT 1 FROM hr_engagements WHERE person_id=$1 AND status='active') THEN 'active' ELSE 'inactive' END WHERE person_id=$2"#,
        bind_count: 2,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_12",
        sql: r#"INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,overtime_hours,note,status,submitted_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,'submitted',$8,$9,$10)"#,
        bind_count: 10,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_13",
        sql: r#"UPDATE hr_time_entries SET work_date=$1,regular_hours=$2,overtime_hours=$3,note=$4,status='submitted',approved_by=NULL,rejection_reason=NULL,version=version+1,updated_at=$5 WHERE id=$6"#,
        bind_count: 6,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_14",
        sql: r#"UPDATE hr_time_entries SET status='rejected',rejection_reason=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_15",
        sql: r#"UPDATE hr_time_entries SET status='approved',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_16",
        sql: r#"INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_17",
        sql: r#"INSERT INTO hr_country_packs(id,country_code,version,status,rules_json,export_formats_json,created_by,created_at,updated_at,adapter_id,source_digest,effective_from,effective_to) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        bind_count: 13,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_18",
        sql: r#"UPDATE hr_country_packs SET status=$1,approval_evidence=$2,validated_by=$3,record_version=record_version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_19",
        sql: r#"INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'final_pay','calculating',$10,$11,$12)"#,
        bind_count: 12,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_20",
        sql: r#"INSERT INTO hr_payroll_final_pay_preparations(id,company_id,task_id,case_id,engagement_id,run_id,reason,prepared_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_21",
        sql: r#"INSERT INTO hr_payroll_final_pay_events(id,preparation_id,task_id,run_id,company_id,action,actor_id,task_version,evidence_json,created_at) VALUES($1,$2,$3,$4,$5,'prepared',$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_22",
        sql: r#"INSERT INTO hr_payroll_advances(id,company_id,engagement_id,person_id,amount_minor,installment_count,first_deduction_period,currency,reason,requested_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_23",
        sql: r#"INSERT INTO hr_payroll_advance_events(id,advance_id,company_id,action,actor_id,advance_version,evidence_json,created_at) VALUES($1,$2,$3,'requested',$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_24",
        sql: r#"UPDATE hr_payroll_advances SET status=$1,version=version+1,approved_by=CASE WHEN $2='approved' THEN $3 ELSE approved_by END,review_reason=$4,reviewed_at=$5,updated_at=$6 WHERE id=$7"#,
        bind_count: 7,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_25",
        sql: r#"INSERT INTO hr_payroll_advance_events(id,advance_id,company_id,action,actor_id,advance_version,evidence_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_26",
        sql: r#"INSERT INTO hr_payroll_advance_repayments(id,advance_id,company_id,sequence,due_period_start,due_period_end,amount_minor,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_27",
        sql: r#"INSERT INTO hr_payroll_account_mappings(id,company_id,pay_group_id,chart_id,accounts_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,'draft',$6,$7,$8)"#,
        bind_count: 8,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_28",
        sql: r#"UPDATE hr_payroll_account_mappings SET status='superseded',version=version+1,updated_at=$1 WHERE company_id=$2 AND pay_group_id=$3 AND status='approved'"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_29",
        sql: r#"UPDATE hr_payroll_account_mappings SET status='approved',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_30",
        sql: r#"UPDATE hr_payroll_runs SET status='approved',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_31",
        sql: r#"INSERT INTO hr_payroll_corrections(id,run_id,company_id,run_version,run_revision,input_digest,totals_json,reason,requested_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        bind_count: 11,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_32",
        sql: r#"INSERT INTO hr_payroll_correction_events(id,correction_id,run_id,company_id,action,actor_id,run_version,evidence_json,created_at) VALUES($1,$2,$3,$4, 'requested',$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_33",
        sql: r#"UPDATE hr_payroll_corrections SET status=$1,version=version+1,reviewed_by=$2,review_reason=$3,reviewed_at=$4,updated_at=$5 WHERE id=$6"#,
        bind_count: 6,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_34",
        sql: r#"INSERT INTO hr_payroll_correction_events(id,correction_id,run_id,company_id,action,actor_id,run_version,evidence_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_35",
        sql: r#"UPDATE hr_payroll_runs SET status='calculating',approved_by=NULL,version=version+1,error_message=$1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_36",
        sql: r#"INSERT INTO hr_payroll_review_exports(id,run_id,run_revision,file_key,checksum_sha256,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_37",
        sql: r#"INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,format_code,total_minor,bank_account_id,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_38",
        sql: r#"INSERT INTO hr_payment_batch_items(id,batch_id,engagement_id,person_id,reference,payee_name,amount_minor,currency) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_39",
        sql: r#"UPDATE hr_payment_batches SET status='draft',retry_requested_by=$1,approved_by=NULL,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5"#,
        bind_count: 5,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_40",
        sql: r#"UPDATE hr_payment_batches SET status='approved',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND status='draft' AND version=$5"#,
        bind_count: 5,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_41",
        sql: r#"INSERT INTO hr_payment_batch_exports(id,company_id,batch_id,batch_version,file_key,checksum_sha256,item_count,total_minor,exported_by,exported_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_42",
        sql: r#"UPDATE hr_payment_batches SET status='exported',file_key=$1,file_checksum=$2,exported_at=$3,version=version+1,updated_at=$4 WHERE id=$5 AND company_id=$6 AND version=$7"#,
        bind_count: 7,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_43",
        sql: r#"INSERT INTO hr_payroll_payment_settlements(id,company_id,batch_id,batch_item_id,bank_transaction_id,journal_id,confirmed_by,confirmed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_44",
        sql: r#"UPDATE hr_payment_batch_items SET status=$1,confirmation_ref=$2,settled_at=$3 WHERE batch_id=$4 AND reference=$5 AND status IN ('outstanding','failed')"#,
        bind_count: 5,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_payroll_stmt_45",
        sql: r#"UPDATE hr_payment_batches SET status=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4"#,
        bind_count: 4,
    }, // native-people-payroll-sql
    NativeStatement {
        kind: "people_roster_stmt_1",
        sql: r#"INSERT INTO people_roster_assignments(id,shift_id,person_id,engagement_id) VALUES($1,$2,$3,$4)"#,
        bind_count: 4,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_2",
        sql: r#"INSERT INTO people_roster_history(id,company_id,department_id,week_id,request_id,record_id,action,actor_id,reason,evidence_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
        bind_count: 11,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_3",
        sql: r#"UPDATE people_roster_requests SET state='invalidated',decision_note='The referenced shift or assignment changed.',version=version+1,updated_at=$1
    WHERE company_id=$2 AND state IN ('pending_coworker','pending_manager') AND id!=$3 AND (target_shift_id=ANY($4::text[]) OR source_assignment_id=ANY($5::text[]) OR target_assignment_id=ANY($6::text[])) RETURNING *"#,
        bind_count: 6,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_4",
        sql: r#"INSERT INTO people_roster_locks(company_id) VALUES($1) ON CONFLICT(company_id) DO UPDATE SET version=people_roster_locks.version+1"#,
        bind_count: 1,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_5",
        sql: r#"UPDATE people_roster_shifts SET local_date=$1,start_time=$2,end_time=$3,starts_at=$4,ends_at=$5,version=version+1 WHERE id=$6"#,
        bind_count: 6,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_6",
        sql: r#"UPDATE people_roster_weeks SET version=version+1,updated_at=$1 WHERE id=$2 RETURNING *"#,
        bind_count: 2,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_7",
        sql: r#"INSERT INTO people_roster_locks(company_id) VALUES($1) ON CONFLICT(company_id) DO UPDATE SET version=people_roster_locks.version+1"#,
        bind_count: 1,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_8",
        sql: r#"INSERT INTO people_roster_templates(id,company_id,department_id,name,start_time,end_time,capacity,active,version,created_by,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT(id) DO UPDATE SET name=excluded.name,start_time=excluded.start_time,end_time=excluded.end_time,capacity=excluded.capacity,active=excluded.active,version=excluded.version,updated_at=excluded.updated_at"#,
        bind_count: 11,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_9",
        sql: r#"INSERT INTO people_roster_weeks(id,company_id,branch_id,department_id,week_start,timezone,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_10",
        sql: r#"INSERT INTO people_roster_shifts(id,week_id,template_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_11",
        sql: r#"UPDATE people_roster_weeks SET state='published',published_by=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_12",
        sql: r#"INSERT INTO people_roster_shifts(id,week_id,template_id,name,local_date,start_time,end_time,starts_at,ends_at,capacity,version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT(id) DO UPDATE SET template_id=excluded.template_id,name=excluded.name,local_date=excluded.local_date,start_time=excluded.start_time,end_time=excluded.end_time,starts_at=excluded.starts_at,ends_at=excluded.ends_at,capacity=excluded.capacity,version=excluded.version"#,
        bind_count: 11,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_13",
        sql: r#"UPDATE people_roster_assignments SET state='released',version=version+1 WHERE shift_id=$1 AND state='assigned'"#,
        bind_count: 1,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_14",
        sql: r#"UPDATE people_roster_weeks SET version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_15",
        sql: r#"UPDATE people_roster_shifts SET state='cancelled',version=version+1 WHERE id=$1"#,
        bind_count: 1,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_16",
        sql: r#"UPDATE people_roster_assignments SET state='released',version=version+1 WHERE shift_id=$1 AND state='assigned'"#,
        bind_count: 1,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_17",
        sql: r#"UPDATE people_roster_weeks SET version=version+1,updated_at=$1 WHERE id=$2 RETURNING *"#,
        bind_count: 2,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_18",
        sql: r#"INSERT INTO people_roster_requests(id,company_id,department_id,kind,requester_person_id,coworker_person_id,source_assignment_id,source_version,target_shift_id,target_assignment_id,target_version,state,reason,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)"#,
        bind_count: 16,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_19",
        sql: r#"UPDATE people_roster_assignments SET state='released',version=version+1 WHERE id=$1"#,
        bind_count: 1,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_20",
        sql: r#"UPDATE people_roster_weeks SET version=version+1,updated_at=$1 WHERE id=ANY($2::text[])"#,
        bind_count: 2,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "people_roster_stmt_21",
        sql: r#"UPDATE people_roster_requests SET state=$1,decision_note=$2,accepted_by=CASE WHEN $3='accept' THEN $4 ELSE accepted_by END,decided_by=$5,version=version+1,updated_at=$6 WHERE id=$7"#,
        bind_count: 7,
    }, // native-people-roster-sql
    NativeStatement {
        kind: "pp_attachments_service_facts_stmt_1",
        sql: r#"UPDATE leave_requests SET version=version+1,evidence_version=evidence_version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_attachments_leave_request",
        sql: r#"UPDATE leave_requests SET version=version+1,evidence_version=evidence_version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_attachments_dynamic_prefix",
        sql: r#"UPDATE "#,
        bind_count: 0,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_attachments_dynamic_mid",
        sql: r#" SET version=version+1"#,
        bind_count: 0,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_attachments_dynamic_mid2",
        sql: r#",updated_at=$1"#,
        bind_count: 1,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_attachments_dynamic_suffix",
        sql: r#" WHERE id=$1 AND version=$2"#,
        bind_count: 2,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_commerce_expense_payment_effects_stmt_1",
        sql: r#"UPDATE finance_expenses SET status=CASE WHEN $1=0 THEN 'reimbursed' ELSE 'posted' END,workflow_stage=CASE WHEN $2=0 THEN 'completed' ELSE 'payment' END,reimbursed_at=CASE WHEN $3=0 THEN $4::bigint ELSE NULL END,version=version+1,updated_at=$5 WHERE id=$6"#,
        bind_count: 6,
    }, // native-pp_commerce_expense_payment_effects-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_1",
        sql: r#"UPDATE expense_ai_proposals SET state='stale',updated_at=$1 WHERE id=$2 AND company_id=$3 AND state='ready'"#,
        bind_count: 3,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_2",
        sql: r#"INSERT INTO expense_ai_proposals(id,company_id,expense_id,payment_draft_id,stage,source_version,evidence_digest,provider_json,state,requested_by,created_at,updated_at,actor_json,finance_policy_version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'queued',$9,$10,$11,$12,$13)"#,
        bind_count: 13,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_3",
        sql: r#"INSERT INTO expense_ai_usage(id,company_id,actor_id,month,state,lease_until,created_at) VALUES($1,$2,$3,$4,'reserved',$5,$6)"#,
        bind_count: 6,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_4",
        sql: r#"UPDATE expense_ai_proposals SET state='running',lease_token=$1,claim_until=$2,attempts=attempts+1,updated_at=$3 WHERE id=$4 AND (state='queued' OR (state='running' AND claim_until<$5))"#,
        bind_count: 5,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_5",
        sql: r#"UPDATE expense_ai_proposals SET claim_until=$1 WHERE id=$2 AND lease_token=$3 AND state='running'"#,
        bind_count: 3,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_6",
        sql: r#"UPDATE expense_ai_proposals SET state='ready',result_json=$1,error=NULL,claim_until=NULL,lease_token=NULL,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_7",
        sql: r#"UPDATE expense_ai_usage SET state='succeeded',lease_until=0,input_tokens=$1,output_tokens=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_8",
        sql: r#"UPDATE expense_ai_proposals SET state=$1,error=$2,result_json=NULL,claim_until=NULL,lease_token=NULL,updated_at=$3 WHERE id=$4 AND lease_token=$5"#,
        bind_count: 5,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_stmt_9",
        sql: r#"UPDATE expense_ai_usage SET state='failed',lease_until=0,input_tokens=$1,output_tokens=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_ai_renew",
        sql: r#"UPDATE expense_ai_proposals SET claim_until=$1 WHERE id=$2 AND lease_token=$3 AND state='running'"#,
        bind_count: 3,
    }, // native-pp_expense_ai-sql
    NativeStatement {
        kind: "pp_expense_payments_stmt_1",
        sql: r#"UPDATE expense_payment_drafts SET payment_date=$1,amount_minor=$2,reference=$3,version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-pp_expense_payments-sql
    NativeStatement {
        kind: "pp_expense_payments_stmt_2",
        sql: r#"UPDATE expense_payment_drafts SET version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_expense_payments-sql
    NativeStatement {
        kind: "pp_expense_payments_stmt_3",
        sql: r#"INSERT INTO expense_payment_drafts(id,expense_id,company_id,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6)"#,
        bind_count: 6,
    }, // native-pp_expense_payments-sql
    NativeStatement {
        kind: "pp_expense_payments_stmt_4",
        sql: r#"UPDATE expense_payment_drafts SET status='confirmed',payment_id=$1,payment_date=$2,amount_minor=$3,reference=$4,version=version+1,updated_at=$5 WHERE id=$6"#,
        bind_count: 6,
    }, // native-pp_expense_payments-sql
    NativeStatement {
        kind: "pp_expense_workflow_stmt_1",
        sql: r#"UPDATE finance_expenses SET category_account_id=$1,amount_minor=$2,tax_minor=$3,accounting_json=$4,version=version+1,updated_at=$5 WHERE id=$6"#,
        bind_count: 6,
    }, // native-pp_expense_workflow-sql
    NativeStatement {
        kind: "pp_expense_workflow_stmt_2",
        sql: r#"UPDATE finance_expenses SET employee_id=$1,engagement_id=$2,branch_id=$3,department_id=$4,workflow_version=2,workflow_stage=$5,status='in_review',accounting_json='{}',approved_by=NULL,rejection_reason=NULL,submitted_at=$6,version=version+1,updated_at=$7 WHERE id=$8"#,
        bind_count: 8,
    }, // native-pp_expense_workflow-sql
    NativeStatement {
        kind: "pp_expense_workflow_stmt_3",
        sql: r#"UPDATE finance_expenses SET workflow_stage='cancelled',status='rejected',cancelled_at=$1,rejection_reason=$2,version=version+1,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-pp_expense_workflow-sql
    NativeStatement {
        kind: "pp_expense_workflow_stmt_4",
        sql: r#"UPDATE finance_expenses SET status='rejected',workflow_stage='draft',accounting_json='{}',approved_by=NULL,rejection_reason=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_expense_workflow-sql
    NativeStatement {
        kind: "pp_expense_workflow_stmt_5",
        sql: r#"UPDATE finance_expenses SET status='approved',workflow_stage='gl_posting',approved_by=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_expense_workflow-sql
    NativeStatement {
        kind: "pp_expense_workflow_stmt_6",
        sql: r#"UPDATE finance_expenses SET status=$1,workflow_stage=$2,rejection_reason=$3,version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-pp_expense_workflow-sql
    NativeStatement {
        kind: "pp_expenses_stmt_1",
        sql: r#"UPDATE finance_expenses SET branch_id=$1,department_id=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_expenses-sql
    NativeStatement {
        kind: "pp_expenses_stmt_2",
        sql: r#"UPDATE finance_expenses SET status=$1,submitted_at=CASE WHEN $2='submit' THEN $3 ELSE submitted_at END,approved_by=CASE WHEN $4='approve' THEN $5 WHEN $6='submit' THEN NULL ELSE approved_by END,rejection_reason=$7,cancelled_at=CASE WHEN $8='cancel' THEN $9 ELSE cancelled_at END,version=version+1,updated_at=$10 WHERE id=$11"#,
        bind_count: 11,
    }, // native-pp_expenses-sql
    NativeStatement {
        kind: "pp_expenses_stmt_3",
        sql: r#"INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-pp_expenses-sql
    NativeStatement {
        kind: "pp_expenses_stmt_4",
        sql: r#"UPDATE finance_expenses SET version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_expenses-sql
    NativeStatement {
        kind: "pp_expenses_stmt_5",
        sql: r#"UPDATE finance_expenses SET status='posted',workflow_stage='payment',invoice_id=$1,journal_id=$2,posted_at=$3,version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-pp_expenses-sql
    NativeStatement {
        kind: "pp_expenses_stmt_6",
        sql: r#"UPDATE finance_expenses SET status='rejected',cancelled_at=$1,rejection_reason=$2,reversal_journal_id=$3,version=version+1,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-pp_expenses-sql
    NativeStatement {
        kind: "pp_expenses_stmt_7",
        sql: r#"INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES($1,$2,$3,'cancel',$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-pp_expenses-sql
    NativeStatement {
        kind: "pp_identity_applied_package_effects_stmt_1",
        sql: r#"INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,position_id,package_id,package_revision,definition,source_kind,scope,case_id,authorizing_assignment_id,valid_from,valid_until,created_at) SELECT 'legacy:'||c.id,c.company_id,c.person_id,c.engagement_id,c.definition->>'positionId',c.package_id,c.package_revision,r.definition,'position',c.review_snapshot,c.id,c.authorizing_assignment_id,c.effective_at,a.valid_until,$2 FROM people_lifecycle_cases c JOIN people_package_revisions r ON r.company_id=c.company_id AND r.package_id=c.package_id AND r.revision=c.package_revision JOIN assignments a ON a.id=c.authorizing_assignment_id WHERE c.id=$1 ON CONFLICT(id) DO NOTHING"#,
        bind_count: 2,
    }, // native-pp_identity_applied_package_effects-sql
    NativeStatement {
        kind: "pp_identity_person_link_effects_stmt_1",
        sql: r#"UPDATE hr_people SET user_id=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_identity_person_link_effects-sql
    NativeStatement {
        kind: "pp_identity_person_link_effects_stmt_2",
        sql: r#"UPDATE employees SET user_id=$1 WHERE person_id=$2"#,
        bind_count: 2,
    }, // native-pp_identity_person_link_effects-sql
    NativeStatement {
        kind: "pp_identity_person_link_effects_stmt_3",
        sql: r#"UPDATE hr_people SET user_id=$1,version=version+1 WHERE id=$2 AND company_id=$3"#,
        bind_count: 3,
    }, // native-pp_identity_person_link_effects-sql
    NativeStatement {
        kind: "pp_identity_person_link_effects_stmt_4",
        sql: r#"UPDATE employees SET user_id=$1 WHERE user_id=$2 AND company_id=$3"#,
        bind_count: 3,
    }, // native-pp_identity_person_link_effects-sql
    NativeStatement {
        kind: "pp_leave_stmt_1",
        sql: r#"UPDATE leave_requests SET start_date=$1,days=$2,handover_note=$3,state=$4,person_id=$5,engagement_id=$6,branch_id=$7,department_id=$8,prepared_by=$9,decided_by=NULL,decision_note=NULL,version=version+1,last_command_id=$10,updated_at=$11 WHERE id=$12"#,
        bind_count: 12,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_leave_stmt_2",
        sql: r#"UPDATE leave_requests SET state=$1,decision_note=$2,decided_by=$3,evidence_version=evidence_version+$4,version=version+1,last_command_id=$5,updated_at=$6 WHERE id=$7"#,
        bind_count: 7,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_leave_stmt_3",
        sql: r#"INSERT INTO leave_cancellation_reviews(id,request_id,company_id,reason,requested_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_leave_stmt_4",
        sql: r#"UPDATE leave_requests SET cancellation_state='pending_review',version=version+1,last_command_id=$1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_leave_stmt_5",
        sql: r#"UPDATE leave_requests SET state='cancelled',decision_note=$1,version=version+1,last_command_id=$2,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_leave_stmt_6",
        sql: r#"UPDATE leave_requests SET state=$1,cancellation_state=$2,version=version+1,last_command_id=$3,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_leave_stmt_7",
        sql: r#"UPDATE leave_cancellation_reviews SET state=$1,decided_by=$2,decision_note=$3,version=version+1,updated_at=$4 WHERE request_id=$5 AND state='pending_approval'"#,
        bind_count: 5,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_leave_stmt_8",
        sql: r#"INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,request_id,kind,units,effective_date,entitlement_year,command_key,note,created_by,created_at)
 SELECT $1||':'||entitlement_year,company_id,engagement_id,leave_type_id,request_id,$2,sum(units)*$3,entitlement_year||'-01-01',entitlement_year,$4||':'||entitlement_year,$5,$6,$7
 FROM leave_balance_entries WHERE request_id=$8 AND company_id=$9 AND kind IN (('usage','usage_reversal','reservation','reservation_release')) GROUP BY company_id,engagement_id,leave_type_id,request_id,entitlement_year HAVING sum(units)<0"#,
        bind_count: 9,
    }, // native-pp_leave-sql
    NativeStatement {
        kind: "pp_operations_people_directory_facts_stmt_1",
        sql: r#"UPDATE employees SET display_name=$1,branch_id=$2,department_id=$3 WHERE person_id=$4"#,
        bind_count: 4,
    }, // native-pp_operations_people_directory_facts-sql
    NativeStatement {
        kind: "pp_operations_people_directory_facts_stmt_2",
        sql: r#"INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at) VALUES($1,$2,$3,$4,$5,$6,'active',$7)"#,
        bind_count: 7,
    }, // native-pp_operations_people_directory_facts-sql
    NativeStatement {
        kind: "pp_operations_people_directory_facts_stmt_3",
        sql: r#"UPDATE employees SET user_id=$1 WHERE person_id=$2"#,
        bind_count: 2,
    }, // native-pp_operations_people_directory_facts-sql
    NativeStatement {
        kind: "pp_operations_people_directory_facts_stmt_4",
        sql: r#"UPDATE employees SET status=$1 WHERE person_id=$2"#,
        bind_count: 2,
    }, // native-pp_operations_people_directory_facts-sql
    NativeStatement {
        kind: "pp_operations_people_lifecycle_worker_effects_stmt_1",
        sql: r#"UPDATE employees SET status='inactive' WHERE company_id=$1 AND person_id=$2"#,
        bind_count: 2,
    }, // native-pp_operations_people_lifecycle_worker_effects-sql
    NativeStatement {
        kind: "pp_operations_people_lifecycle_worker_effects_stmt_2",
        sql: r#"UPDATE employees SET branch_id=$3,department_id=$4,status='active' WHERE company_id=$1 AND person_id=$2"#,
        bind_count: 4,
    }, // native-pp_operations_people_lifecycle_worker_effects-sql
    NativeStatement {
        kind: "pp_operations_people_lifecycle_worker_effects_stmt_3",
        sql: r#"INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-pp_operations_people_lifecycle_worker_effects-sql
    NativeStatement {
        kind: "pp_organization_defaults_stmt_1",
        sql: r#"INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,effective_from,timezone,workweek_json,created_by,created_at) VALUES($1,$2,$3,$4,$5,'active',$6,'UTC','[1,2,3,4,5]'::jsonb,$7,$8)"#,
        bind_count: 8,
    }, // native-pp_organization_defaults-sql
    NativeStatement {
        kind: "pp_organization_defaults_stmt_2",
        sql: r#"INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,accrual_units,employee_eligible,contractor_eligible) VALUES($1,$2,'annual',$3,0,true,false)"#,
        bind_count: 3,
    }, // native-pp_organization_defaults-sql
    NativeStatement {
        kind: "pp_organization_defaults_stmt_3",
        sql: r#"UPDATE hr_engagements SET pay_group_id=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_organization_defaults-sql
    NativeStatement {
        kind: "pp_organization_defaults_stmt_4",
        sql: r#"INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,'monthly',$4,0,0,$5,'[]','draft',$6,$7,$8)"#,
        bind_count: 8,
    }, // native-pp_organization_defaults-sql
    NativeStatement {
        kind: "pp_organization_defaults_stmt_5",
        sql: r#"UPDATE leave_policy_assignments SET effective_to=$1,version=version+1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_organization_defaults-sql
    NativeStatement {
        kind: "pp_organization_defaults_stmt_6",
        sql: r#"INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)"#,
        bind_count: 7,
    }, // native-pp_organization_defaults-sql
    NativeStatement {
        kind: "pp_overtime_stmt_1",
        sql: r#"INSERT INTO overtime_requests(id,company_id,engagement_id,person_id,branch_id,department_id,created_by,mode,timezone,start_local,end_local,starts_at,ends_at,break_minutes,minutes,reason,late_explanation,roster_shift_id,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)"#,
        bind_count: 20,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_overtime_stmt_2",
        sql: r#"UPDATE overtime_requests SET start_local=$1,end_local=$2,timezone=$3,starts_at=$4,ends_at=$5,break_minutes=$6,minutes=$7,reason=$8,late_explanation=$9,roster_shift_id=$10 WHERE id=$11"#,
        bind_count: 11,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_overtime_stmt_3",
        sql: r#"UPDATE overtime_requests SET planned_snapshot=$1::jsonb,decision_note='' WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_overtime_stmt_4",
        sql: r#"INSERT INTO overtime_payroll_handoffs(id,request_id,company_id,engagement_id,request_version,work_date,minutes,timezone,approved_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_overtime_stmt_5",
        sql: r#"UPDATE overtime_requests SET decision_note=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_overtime_stmt_6",
        sql: r#"UPDATE overtime_requests SET state=$1,version=$2,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_overtime_stmt_7",
        sql: r#"UPDATE hr_engagements SET self_service_top_level=$1,version=version+1 WHERE id=$2 AND company_id=$3"#,
        bind_count: 3,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_overtime_stmt_8",
        sql: r#"INSERT INTO overtime_eligibility(company_id,engagement_id,eligible,reason,updated_by,updated_at,version) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(company_id,engagement_id) DO UPDATE SET eligible=excluded.eligible,reason=excluded.reason,updated_by=excluded.updated_by,updated_at=excluded.updated_at,version=excluded.version"#,
        bind_count: 7,
    }, // native-pp_overtime-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_1",
        sql: r#"INSERT INTO hr_payroll_adjustment_revisions(id,run_id,revision,taxable_arrears_minor,non_taxable_arrears_minor,social_security_wage_minor,time_entries_json,reason,evidence_ref,created_by,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_2",
        sql: r#"INSERT INTO hr_payroll_adjustment_time_claims(run_id,source_revision,time_entry_id) VALUES($1,$2,$3)"#,
        bind_count: 3,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_3",
        sql: r#"INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,adjustment_of_run_id,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'THB','production','calculating',$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_4",
        sql: r#"INSERT INTO hr_payroll_adjustments(run_id,company_id,original_run_id,original_item_id,engagement_id,source_ref,source_snapshot_json,source_digest,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
        bind_count: 10,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_5",
        sql: r#"UPDATE hr_payroll_adjustment_time_claims SET active=false WHERE run_id=$1 AND active"#,
        bind_count: 1,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_6",
        sql: r#"UPDATE hr_payroll_adjustments SET source_revision=source_revision+1 WHERE run_id=$1"#,
        bind_count: 1,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_7",
        sql: r#"UPDATE hr_payroll_runs SET status='calculating',input_digest=NULL,totals_json='{}',version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_8",
        sql: r#"DELETE FROM hr_payroll_items WHERE run_id=$1"#,
        bind_count: 1,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_9",
        sql: r#"UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=$1,input_digest=$2,version=version+1,error_message=NULL,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_adjustments_stmt_10",
        sql: r#"INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES($1,$2,$3,'employee',$4,$5,$6,$7,$8,$9,'THB',$10)"#,
        bind_count: 10,
    }, // native-pp_payroll_adjustments-sql
    NativeStatement {
        kind: "pp_payroll_other_remittance_stmt_1",
        sql: r#"INSERT INTO hr_payroll_statutory_remittances(id,company_id,run_id,kind,amount_minor,liability_account_id,bank_account_id,bank_transaction_id,journal_id,bank_reference,recorded_by,recorded_at,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)"#,
        bind_count: 13,
    }, // native-pp_payroll_other_remittance-sql
    NativeStatement {
        kind: "pp_payroll_readiness_stmt_1",
        sql: r#"UPDATE hr_payroll_runs SET status='in_review',submitted_by=$1,submitted_digest=$2,submitted_at=$3,version=version+1,updated_at=$4 WHERE id=$5 AND company_id=$6 AND status='preview' AND version=$7"#,
        bind_count: 7,
    }, // native-pp_payroll_readiness-sql
    NativeStatement {
        kind: "pp_payroll_readiness_stmt_2",
        sql: r#"UPDATE hr_pay_groups SET schedule_enabled=$1,pay_delay_days=$2,schedule_anchor_date=$3,schedule_version=schedule_version+1,updated_at=$4 WHERE id=$5 AND company_id=$6 AND schedule_version=$7"#,
        bind_count: 7,
    }, // native-pp_payroll_readiness-sql
    NativeStatement {
        kind: "pp_payroll_readiness_stmt_3",
        sql: r#"INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) SELECT $1,$2,$3,NULL,$4,$5,$6,$7,'preview','draft','system:payroll-schedule',$8,$9 WHERE NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=$10 AND pay_group_id=$11 AND period_start=$12 AND period_end=$13 AND mode!='final_pay' AND adjustment_of_run_id IS NULL AND status!='cancelled') ON CONFLICT DO NOTHING"#,
        bind_count: 13,
    }, // native-pp_payroll_readiness-sql
    NativeStatement {
        kind: "pp_payroll_readiness_stmt_4",
        sql: r#"UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=$1,input_digest=$2,
          submitted_by=NULL,submitted_digest=NULL,submitted_at=NULL,version=version+1,updated_at=$3,error_message=NULL WHERE id=$4"#,
        bind_count: 4,
    }, // native-pp_payroll_readiness-sql
    NativeStatement {
        kind: "pp_payroll_readiness_stmt_5",
        sql: r#"INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,
            gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-pp_payroll_readiness-sql
    NativeStatement {
        kind: "pp_payroll_readiness_stmt_6",
        sql: r#"UPDATE hr_payroll_runs SET status='failed',error_message=$1,updated_at=$2
        WHERE id=$3 AND status IN ('draft','failed') AND created_by='system:payroll-schedule'"#,
        bind_count: 3,
    }, // native-pp_payroll_readiness-sql
    NativeStatement {
        kind: "pp_payroll_recoveries_stmt_1",
        sql: r#"INSERT INTO hr_payroll_recovery_events(id,recovery_id,company_id,action,actor_id,recovery_version,evidence_json,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-pp_payroll_recoveries-sql
    NativeStatement {
        kind: "pp_payroll_recoveries_stmt_2",
        sql: r#"INSERT INTO hr_payroll_recoveries(id,company_id,original_run_id,original_item_id,engagement_id,person_id,employee_user_id,source_ref,source_snapshot_json,source_digest,policy_json,policy_digest,amount_minor,currency,installment_count,first_deduction_period,reason,evidence_ref,requested_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,\'THB\',$14,$15,$16,$17,$18,$19,$20)"#,
        bind_count: 20,
    }, // native-pp_payroll_recoveries-sql
    NativeStatement {
        kind: "pp_payroll_recoveries_stmt_3",
        sql: r#"UPDATE hr_payroll_recoveries SET status=$1,version=version+1,consent_by=$2,consent_at=$3,consent_html=$4,consent_sha256=$5,updated_at=$6 WHERE id=$7"#,
        bind_count: 7,
    }, // native-pp_payroll_recoveries-sql
    NativeStatement {
        kind: "pp_payroll_recoveries_stmt_4",
        sql: r#"UPDATE hr_payroll_recoveries SET status=$1,version=version+1,reviewed_by=$2,legal_basis_ref=$3,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-pp_payroll_recoveries-sql
    NativeStatement {
        kind: "pp_payroll_recoveries_stmt_5",
        sql: r#"INSERT INTO hr_payroll_advances(id,company_id,engagement_id,person_id,status,version,amount_minor,installment_count,first_deduction_period,currency,reason,requested_by,approved_by,review_reason,created_at,reviewed_at,updated_at,recovery_id) VALUES($1,$2,$3,$4,'approved',2,$5,$6,$7,'THB',$8,$9,$10,$11,$12,$13,$14,$15)"#,
        bind_count: 15,
    }, // native-pp_payroll_recoveries-sql
    NativeStatement {
        kind: "pp_payroll_recoveries_stmt_6",
        sql: r#"UPDATE hr_payroll_recoveries SET status='recognized',version=version+1,journal_id=$1,advance_id=$2,recognized_by=$3,updated_at=$4 WHERE id=$5"#,
        bind_count: 5,
    }, // native-pp_payroll_recoveries-sql
    NativeStatement {
        kind: "pp_payroll_recoveries_stmt_7",
        sql: r#"INSERT INTO hr_payroll_advance_repayments(id,advance_id,company_id,sequence,due_period_start,due_period_end,amount_minor,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)"#,
        bind_count: 8,
    }, // native-pp_payroll_recoveries-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_1",
        sql: r#"UPDATE hr_people SET legal_name=$1,preferred_name=$2,email=$3,phone=$4,work_email=$5,work_phone=$6,country_code=$7,version=version+1,updated_at=$8 WHERE id=$9"#,
        bind_count: 9,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_2",
        sql: r#"UPDATE hr_engagements SET branch_id=$1,department_id=$2,job_title=$3,worker_type=$4,employment_type=$5,start_date=$6,currency=$7,manager_person_id=$8,version=version+1,updated_at=$9 WHERE id=$10"#,
        bind_count: 10,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_3",
        sql: r#"INSERT INTO hr_people(id,company_id,legal_name,preferred_name,email,phone,work_email,work_phone,country_code,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
        bind_count: 12,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_4",
        sql: r#"INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,manager_person_id,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,'onboarding',$8,$9,$10,$11,$12,$13,$14)"#,
        bind_count: 14,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_5",
        sql: r#"UPDATE hr_engagements SET level_id=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_6",
        sql: r#"UPDATE hr_people SET user_id=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_7",
        sql: r#"UPDATE hr_engagements SET position_id=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_8",
        sql: r#"INSERT INTO staff_organization_history(id,company_id,engagement_id,before_json,after_json,actor_id,reason,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_9",
        sql: r#"UPDATE hr_people SET status=$1,version=version+1,updated_at=$2 WHERE id=$3"#,
        bind_count: 3,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_directory_stmt_10",
        sql: r#"UPDATE hr_engagements SET status=$1,version=version+1,updated_at=$2 WHERE person_id=$3 AND status!='ended'"#,
        bind_count: 3,
    }, // native-pp_people_directory-sql
    NativeStatement {
        kind: "pp_people_evaluation_ai_stmt_1",
        sql: r#"INSERT INTO hr_evaluation_ai_jobs(id,company_id,actor_json,query_json,source_hash,provider_json,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        bind_count: 9,
    }, // native-pp_people_evaluation_ai-sql
    NativeStatement {
        kind: "pp_people_evaluation_ai_stmt_2",
        sql: r#"UPDATE hr_evaluation_ai_jobs SET state='running',lease_token=$1,claim_until=$2,attempts=attempts+1,updated_at=$3 WHERE id=$4 AND attempts<3 AND (state='queued' OR (state='running' AND claim_until<$5)) RETURNING *"#,
        bind_count: 5,
    }, // native-pp_people_evaluation_ai-sql
    NativeStatement {
        kind: "pp_people_evaluation_ai_stmt_3",
        sql: r#"UPDATE hr_evaluation_ai_jobs SET claim_until=$1 WHERE id=$2 AND lease_token=$3 AND state='running'"#,
        bind_count: 3,
    }, // native-pp_people_evaluation_ai-sql
    NativeStatement {
        kind: "pp_people_evaluation_ai_stmt_4",
        sql: r#"UPDATE hr_evaluation_ai_jobs SET state='ready',result_json=$1,error=NULL,lease_token=NULL,claim_until=NULL,updated_at=$2 WHERE id=$3 AND lease_token=$4"#,
        bind_count: 4,
    }, // native-pp_people_evaluation_ai-sql
    NativeStatement {
        kind: "pp_people_evaluation_ai_stmt_5",
        sql: r#"UPDATE hr_evaluation_ai_jobs SET state=$1,error=$2,lease_token=NULL,claim_until=NULL,updated_at=$3 WHERE id=$4 AND lease_token=$5"#,
        bind_count: 5,
    }, // native-pp_people_evaluation_ai-sql
    NativeStatement {
        kind: "pp_people_evaluation_ai_stmt_6",
        sql: r#"UPDATE hr_evaluation_ai_jobs SET state='failed',error='AI was interrupted. Continue manually.',lease_token=NULL,claim_until=NULL,updated_at=$1 WHERE state='running' AND claim_until<$2 AND attempts>=3"#,
        bind_count: 2,
    }, // native-pp_people_evaluation_ai-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_1",
        sql: r#"UPDATE hr_engagements SET status=$2,end_date=CASE WHEN $2='ended' THEN $3 ELSE end_date END,version=version+1,updated_at=$4 WHERE id=$1"#,
        bind_count: 4,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_2",
        sql: r#"UPDATE people_access_cutoffs SET restored_at=$3 WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3"#,
        bind_count: 3,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_3",
        sql: r#"UPDATE people_applied_packages SET ended_at=$3 WHERE company_id=$1 AND person_id=$2 AND source_kind='position' AND ended_at IS NULL"#,
        bind_count: 3,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_4",
        sql: r#"UPDATE people_lifecycle_cases SET status='completed' WHERE id=$1"#,
        bind_count: 1,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_5",
        sql: r#"UPDATE hr_engagements SET position_id=$2,branch_id=$3,department_id=$4,level_id=$5,manager_person_id=$6,job_title=$7,status='active',version=version+1,updated_at=$8 WHERE id=$1"#,
        bind_count: 8,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_6",
        sql: r#"UPDATE hr_engagements SET pay_group_id=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_7",
        sql: r#"INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,'monthly',$4,0,0,$5,'[]','draft',$6,$7,$7) ON CONFLICT(id) DO NOTHING"#,
        bind_count: 7,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_8",
        sql: r#"UPDATE leave_policy_assignments SET effective_to=$1,version=version+1 WHERE id=$2"#,
        bind_count: 2,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_9",
        sql: r#"INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,effective_from,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(id) DO NOTHING"#,
        bind_count: 7,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_10",
        sql: r#"UPDATE people_lifecycle_cases SET status='completed',version=version+1,updated_at=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_lifecycle_jobs_stmt_11",
        sql: r#"UPDATE people_lifecycle_cases SET attempts=attempts+1,next_attempt_at=$2+LEAST(3600000,1000*power(2,LEAST(attempts,12)))::bigint,last_error=$3 WHERE id=$1 AND status='scheduled'"#,
        bind_count: 3,
    }, // native-pp_people_lifecycle_jobs-sql
    NativeStatement {
        kind: "pp_people_templates_stmt_1",
        sql: r#"UPDATE people_holding_templates SET version=version+1,name=$1,definition=$2,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // native-pp_people_templates-sql
    NativeStatement {
        kind: "pp_people_templates_stmt_2",
        sql: r#"INSERT INTO people_holding_templates(id,holding_id,name,definition,created_by,updated_at) VALUES($1,$2,$3,$4,$5,$6)"#,
        bind_count: 6,
    }, // native-pp_people_templates-sql
    NativeStatement {
        kind: "pp_people_templates_stmt_3",
        sql: r#"INSERT INTO people_lifecycle_settings(company_id,version,definition) VALUES($1,1,$2) ON CONFLICT(company_id) DO UPDATE SET version=people_lifecycle_settings.version+1,definition=excluded.definition"#,
        bind_count: 2,
    }, // native-pp_people_templates-sql
    NativeStatement {
        kind: "pp_people_templates_stmt_4",
        sql: r#"INSERT INTO people_template_adoptions(company_id,template_id,template_version,definition) VALUES($1,$2,$3,$4) ON CONFLICT(company_id,template_id) DO UPDATE SET template_version=excluded.template_version,version=people_template_adoptions.version+1,definition=excluded.definition"#,
        bind_count: 4,
    }, // native-pp_people_templates-sql
    NativeStatement {
        kind: "tech_provision_stmt_1",
        sql: r#"INSERT INTO people_provision_operations(id,company_id,definition,evidence,status,created_by,assignment_id,session_id,created_at,updated_at) VALUES($1,$2,$3,$4,'queued',$5,$6,$7,$8,$8)"#,
        bind_count: 8,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_2",
        sql: r#"INSERT INTO people_applied_packages(id,company_id,person_id,engagement_id,position_id,package_id,package_revision,definition,source_kind,scope,operation_id,authorizing_assignment_id,valid_from,valid_until,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$13)"#,
        bind_count: 14,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_3",
        sql: r#"INSERT INTO people_provision_grants(id,operation_id,application_id,definition,review_id) VALUES($1,$2,$3,$4,$5)"#,
        bind_count: 5,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_4",
        sql: r#"UPDATE people_provision_operations SET steps=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_5",
        sql: r#"UPDATE people_provision_grants SET assignment_id=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_6",
        sql: r#"UPDATE people_provision_operations SET status=$2,steps=$3,last_error=$4 WHERE id=$1"#,
        bind_count: 4,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_7",
        sql: r#"UPDATE people_applied_packages SET ended_at=$3 WHERE company_id=$1 AND person_id=$2 AND operation_id IS DISTINCT FROM $4 AND ended_at IS NULL"#,
        bind_count: 4,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_8",
        sql: r#"UPDATE people_access_cutoffs SET restored_at=$3 WHERE company_id=$1 AND person_id=$2 AND cancelled_at IS NULL AND restored_at IS NULL AND effective_at<=$3"#,
        bind_count: 3,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_9",
        sql: r#"UPDATE people_preparation_tasks t SET status='completed',version=t.version+1,updated_at=$3 FROM people_lifecycle_cases lc WHERE t.case_id=lc.id AND lc.company_id=$1 AND lc.engagement_id=$2 AND lc.workflow='placement' AND lc.status='completed' AND lc.kind IN ('onboarding','transfer') AND t.title='Prepare access for employee placement' AND t.status='pending'"#,
        bind_count: 3,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_10",
        sql: r#"UPDATE people_applied_packages SET ended_at=$2 WHERE id=$1"#,
        bind_count: 2,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_11",
        sql: r#"UPDATE people_applied_packages SET ended_at=$2 WHERE id=ANY($1::text[]) AND ended_at IS NULL"#,
        bind_count: 2,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_12",
        sql: r#"INSERT INTO people_preparation_tasks(id,company_id,kind,title,updated_at,privileged,application_id) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING"#,
        bind_count: 7,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_13",
        sql: r#"UPDATE people_provision_operations SET status='completed',steps=$2,version=version+1,updated_at=$3,last_error=NULL WHERE id=$1"#,
        bind_count: 3,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_14",
        sql: r#"UPDATE people_provision_operations SET status='blocked',last_error=$2,version=version+1,updated_at=$3 WHERE id=$1"#,
        bind_count: 3,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "tech_provision_stmt_15",
        sql: r#"UPDATE people_provision_operations SET status='queued',session_id=$2,version=version+1,last_error=NULL,updated_at=$3 WHERE id=$1"#,
        bind_count: 3,
    }, // native-tech-provision-sql
    NativeStatement {
        kind: "pp_attachments_record_version",
        sql: r#"UPDATE finance_expenses SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_attachments_payment_version",
        sql: r#"UPDATE expense_payment_drafts SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // native-pp_attachments_service_facts-sql
    NativeStatement {
        kind: "pp_attach_sensitive_evaluation",
        sql: r#"UPDATE hr_evaluation_assessments SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_sensitive_person",
        sql: r#"UPDATE hr_people SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_sensitive_lifecycle_case",
        sql: r#"UPDATE people_lifecycle_cases SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_sensitive_clearance_task",
        sql: r#"UPDATE people_preparation_tasks SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_sensitive_payroll_run",
        sql: r#"UPDATE hr_payroll_runs SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_sensitive_payroll_advance",
        sql: r#"UPDATE hr_payroll_advances SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_sensitive_final_pay",
        sql: r#"UPDATE hr_payroll_final_pay_preparations SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_finance_overtime",
        sql: r#"UPDATE overtime_requests SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_finance_time_entry",
        sql: r#"UPDATE hr_time_entries SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_attach_finance_provision",
        sql: r#"UPDATE people_provision_operations SET version=version+1,updated_at=$1 WHERE id=$2 AND version=$3"#,
        bind_count: 3,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_approval_scope_leave",
        sql: r#"UPDATE leave_requests SET branch_id=$1,department_id=$2,version=version+1,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_approval_scope_expense",
        sql: r#"UPDATE finance_expenses SET branch_id=$1,department_id=$2,version=version+1,updated_at=$3 WHERE id=$4"#,
        bind_count: 4,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_approval_revision_leave",
        sql: r#"UPDATE leave_requests SET version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_approval_revision_overtime",
        sql: r#"UPDATE overtime_requests SET version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_approval_revision_expense",
        sql: r#"UPDATE finance_expenses SET version=version+1,updated_at=$1 WHERE id=$2"#,
        bind_count: 2,
    }, // owner-dynamic-writes
    NativeStatement {
        kind: "pp_organization_self_ceo",
        sql: r#"UPDATE hr_engagements e SET position_id=p.id,level_id=p.level_id,manager_person_id=NULL,branch_id=NULL,department_id=NULL,job_title=p.title,version=e.version+1,updated_at=$1 FROM (SELECT "id","company_id","branch_id","department_id","title","level_id","reports_to_position_id","is_ceo","active","version","created_by","created_at","updated_at","requirements","roster_required" FROM organization_positions) p WHERE p.id=$2 AND p.company_id=e.company_id AND e.company_id=$3 AND e.person_id=$4 AND e.status='active'"#,
        bind_count: 4,
    }, // owner-dynamic-writes
];

/// The complete allow-listed set of native write kinds. Only these may cross
/// the HTTP ingress; any other kind is rejected before the backend is called.
pub fn statement(kind: &str) -> Option<&'static NativeStatement> {
    STATEMENTS.iter().find(|item| item.kind == kind)
}

/// Number of allow-listed kinds, exposed for contract tests.
pub fn statement_count() -> usize {
    STATEMENTS.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_statement_is_well_formed() {
        assert_eq!(statement_count(), 278);
        for item in STATEMENTS {
            assert!(!item.kind.is_empty() && !item.sql.is_empty());
            assert_eq!(
                item.sql.matches('$').count() >= item.bind_count,
                true,
                "{}",
                item.kind
            );
            assert!(statement(item.kind).is_some());
        }
    }

    #[test]
    fn unknown_kinds_are_not_admitted() {
        assert!(statement("not_a_native_statement").is_none());
        assert!(statement("").is_none());
    }
}
