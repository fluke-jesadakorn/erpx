//! PostgreSQL owner adapter for every native Work write. All SQL lives here in
//! the owner crate. The caller owns one transaction; this adapter executes the
//! typed kind and writes the command receipt, audit event and outbox event in
//! that same unit (CQRS command side).
use crate::application::owner_write::{OwnerWriteCommand, OwnerWritePort};
use crate::domain::owner_write_registry::{
    WP_CAL_ANNOUNCEMENT_DATES, WP_CAL_ANNOUNCEMENT_STATUS, WP_CAL_ANNOUNCEMENT_UPSERT,
    WP_CAL_APPOINTMENT_CLONE, WP_CAL_APPOINTMENT_STATUS, WP_CAL_APPOINTMENT_UPDATE,
    WP_CAL_APPOINTMENT_UPSERT, WP_CAL_APPOINTMENT_VERSION_BUMP, WP_CAL_ATTENDEE_CLONE,
    WP_CAL_ATTENDEE_DELETE, WP_CAL_ATTENDEE_INSERT, WP_CAL_ATTENDEE_REMINDER_UPDATE,
    WP_CAL_ATTENDEE_RESPONSE_UPDATE, WP_CAL_HISTORY_INSERT, WP_CAL_OCCURRENCE_EXCEPTION_CANCEL,
    WP_CAL_OCCURRENCE_EXCEPTION_UPSERT, WP_CAL_OCCURRENCE_RESPONSE_UPSERT,
    WP_CAL_PARTICIPANT_CLONE, WP_CAL_PARTICIPANT_DELETE, WP_CAL_PARTICIPANT_INSERT,
    WP_CAL_REMINDER_DELIVERY_DELETE, WP_CAL_SERIES_DELETE, WP_CAL_SERIES_INSERT,
    WP_CAL_SERIES_UPSERT, WP_CAL_SERIES_UNTIL, WP_DAILY_PLAN_WRITE, WP_PAGE_LAYOUT_RECEIPT_SAVE,
    WP_PAGE_LAYOUT_SAVE, WP_PROJECTION_CHECKPOINT, WP_PROJECTION_RECEIPT, WP_QUICK_ACCESS_RECEIPT_SAVE,
    WP_QUICK_ACCESS_SAVE, WP_REMINDER_AUTHORIZATION_INSERT, WP_REMINDER_DELIVERY_CLAIM,
    WP_REMINDER_DELIVERY_UPSERT, WP_WORK_ITEM_UPSERT, returning,
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use sqlx::{Connection, Database, PgConnection, Postgres, Row, query::Query};

pub type OwnerWriteError = PgTransactionError<BusinessError>;

type BoundQuery<'q> = Query<'q, Postgres, <Postgres as Database>::Arguments>;

pub struct PgOwnerWrite<'c> {
    connection: &'c mut PgConnection,
}

impl<'c> PgOwnerWrite<'c> {
    pub fn new(connection: &'c mut PgConnection) -> Self {
        Self { connection }
    }

    fn sql(kind: &str) -> Result<&'static str, BusinessError> {
        Ok(match kind {
            WP_CAL_OCCURRENCE_EXCEPTION_UPSERT =>
                "INSERT INTO calendar_occurrence_exceptions(company_id,appointment_id,occurrence_key,start_local,end_local) VALUES($1,$2,$3,$4,$5) ON CONFLICT(appointment_id,occurrence_key) DO UPDATE SET start_local=excluded.start_local,end_local=excluded.end_local,cancelled=false,version=calendar_occurrence_exceptions.version+1",
            WP_CAL_APPOINTMENT_VERSION_BUMP =>
                "UPDATE calendar_appointments SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3",
            WP_CAL_SERIES_UNTIL =>
                "UPDATE calendar_series SET end_kind='until',until_date=$1,occurrence_count=NULL,version=version+1 WHERE appointment_id=$2",
            WP_CAL_APPOINTMENT_CLONE =>
                "INSERT INTO calendar_appointments(id,company_id,branch_id,department_id,organizer_person_id,title,description,starts_at,ends_at,timezone,location,meeting_url,private,guests,status,version,created_by,created_at,updated_at,all_day,start_date,end_date) SELECT $1,company_id,branch_id,department_id,organizer_person_id,title,description,$2,$3,$4,location,meeting_url,private,guests,status,1,created_by,$5,$6,$7,$8,$9 FROM calendar_appointments WHERE id=$10 AND company_id=$11",
            WP_CAL_SERIES_INSERT =>
                "INSERT INTO calendar_series(appointment_id,company_id,frequency,interval_count,weekdays,end_kind,occurrence_count,until_date) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            WP_CAL_ATTENDEE_CLONE =>
                "INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes) SELECT company_id,$1,user_id,person_id,attendance,CASE WHEN attendance='organizer' THEN 'accepted' ELSE 'pending' END,reminder_minutes FROM calendar_attendees WHERE appointment_id=$2 AND company_id=$3",
            WP_CAL_PARTICIPANT_CLONE =>
                "INSERT INTO calendar_appointment_participants(company_id,appointment_id,person_id) SELECT company_id,$1,person_id FROM calendar_appointment_participants WHERE appointment_id=$2 AND company_id=$3",
            WP_CAL_APPOINTMENT_UPDATE =>
                "UPDATE calendar_appointments SET starts_at=$1,ends_at=$2,timezone=$3,start_date=$4,end_date=$5,version=version+1,updated_at=$6 WHERE id=$7 AND company_id=$8",
            WP_CAL_ANNOUNCEMENT_DATES =>
                "UPDATE calendar_announcements SET start_date=$1,end_date=$2,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5",
            WP_CAL_APPOINTMENT_UPSERT =>
                "INSERT INTO calendar_appointments(id,company_id,branch_id,department_id,organizer_person_id,title,description,starts_at,ends_at,timezone,location,meeting_url,private,guests,status,version,created_by,created_at,updated_at,all_day,start_date,end_date) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14::jsonb,'scheduled',$15,$16,$17,$18,$19,$20,$21) ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,organizer_person_id=excluded.organizer_person_id,title=excluded.title,description=excluded.description,starts_at=excluded.starts_at,ends_at=excluded.ends_at,timezone=excluded.timezone,location=excluded.location,meeting_url=excluded.meeting_url,private=excluded.private,guests=excluded.guests,version=excluded.version,updated_at=excluded.updated_at,all_day=excluded.all_day,start_date=excluded.start_date,end_date=excluded.end_date",
            WP_CAL_ATTENDEE_DELETE => "DELETE FROM calendar_attendees WHERE appointment_id=$1 AND company_id=$2",
            WP_CAL_ATTENDEE_INSERT =>
                "INSERT INTO calendar_attendees(company_id,appointment_id,user_id,person_id,attendance,response,reminder_minutes,response_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            WP_CAL_PARTICIPANT_DELETE => "DELETE FROM calendar_appointment_participants WHERE appointment_id=$1 AND company_id=$2",
            WP_CAL_PARTICIPANT_INSERT =>
                "INSERT INTO calendar_appointment_participants(company_id,appointment_id,person_id) VALUES($1,$2,$3)",
            WP_CAL_SERIES_DELETE => "DELETE FROM calendar_series WHERE appointment_id=$1 AND company_id=$2",
            WP_CAL_SERIES_UPSERT =>
                "INSERT INTO calendar_series(appointment_id,company_id,frequency,interval_count,weekdays,end_kind,occurrence_count,until_date) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(appointment_id) DO UPDATE SET frequency=excluded.frequency,interval_count=excluded.interval_count,weekdays=excluded.weekdays,end_kind=excluded.end_kind,occurrence_count=excluded.occurrence_count,until_date=excluded.until_date,version=calendar_series.version+1",
            WP_CAL_OCCURRENCE_RESPONSE_UPSERT =>
                "INSERT INTO calendar_occurrence_responses(company_id,appointment_id,occurrence_key,user_id,response,responded_at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(appointment_id,occurrence_key,user_id) DO UPDATE SET response=excluded.response,responded_at=excluded.responded_at",
            WP_CAL_ATTENDEE_RESPONSE_UPDATE =>
                "UPDATE calendar_attendees SET response=$1,response_at=$2 WHERE company_id=$3 AND appointment_id=$4 AND user_id=$5",
            WP_CAL_ATTENDEE_REMINDER_UPDATE =>
                "UPDATE calendar_attendees SET reminder_minutes=$1 WHERE company_id=$2 AND appointment_id=$3 AND user_id=$4 RETURNING user_id",
            WP_CAL_REMINDER_DELIVERY_DELETE =>
                "DELETE FROM calendar_reminder_deliveries WHERE company_id=$1 AND appointment_id=$2 AND user_id=$3 AND delivered_at IS NULL",
            WP_CAL_OCCURRENCE_EXCEPTION_CANCEL =>
                "INSERT INTO calendar_occurrence_exceptions(company_id,appointment_id,occurrence_key,cancelled) VALUES($1,$2,$3,true) ON CONFLICT(appointment_id,occurrence_key) DO UPDATE SET cancelled=true,version=calendar_occurrence_exceptions.version+1",
            WP_CAL_APPOINTMENT_STATUS =>
                "UPDATE calendar_appointments SET status=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4",
            WP_CAL_ANNOUNCEMENT_UPSERT =>
                "INSERT INTO calendar_announcements(id,company_id,branch_id,department_id,title,content,start_date,end_date,status,version,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) ON CONFLICT(id) DO UPDATE SET branch_id=excluded.branch_id,department_id=excluded.department_id,title=excluded.title,content=excluded.content,start_date=excluded.start_date,end_date=excluded.end_date,version=excluded.version,updated_at=excluded.updated_at",
            WP_CAL_ANNOUNCEMENT_STATUS =>
                "UPDATE calendar_announcements SET status=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4",
            WP_CAL_HISTORY_INSERT =>
                "INSERT INTO calendar_history(id,company_id,source,record_id,action,actor_id,version,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
            WP_REMINDER_DELIVERY_UPSERT =>
                "INSERT INTO calendar_reminder_deliveries(company_id,appointment_id,occurrence_key,user_id,due_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(appointment_id,occurrence_key,user_id) DO UPDATE SET due_at=excluded.due_at,delivered_at=CASE WHEN calendar_reminder_deliveries.due_at=excluded.due_at THEN calendar_reminder_deliveries.delivered_at ELSE NULL END",
            WP_REMINDER_DELIVERY_CLAIM =>
                "UPDATE calendar_reminder_deliveries SET delivered_at=$1 WHERE appointment_id=$2 AND occurrence_key=$3 AND user_id=$4 AND due_at=$5 AND delivered_at IS NULL RETURNING user_id",
            WP_REMINDER_AUTHORIZATION_INSERT =>
                "INSERT INTO calendar_reminder_execution_authorizations(command_id,appointment_id,recipient_id,company_id,actor_id,assignment_id,action,authority_json,source_hash,source_version,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
            WP_DAILY_PLAN_WRITE =>
                "INSERT INTO staff_daily_plans(company_id,user_id,plan_date,items_json,version,last_command_id,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(company_id,user_id,plan_date) DO UPDATE SET items_json=excluded.items_json,version=excluded.version,last_command_id=excluded.last_command_id,updated_at=excluded.updated_at WHERE staff_daily_plans.version=$8",
            WP_PAGE_LAYOUT_SAVE =>
                "INSERT INTO page_layouts(company_id,owner_id,workspace_base,page_key,layout_json,version,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(company_id,owner_id,workspace_base,page_key) DO UPDATE SET layout_json=excluded.layout_json,version=excluded.version,updated_at=excluded.updated_at",
            WP_PAGE_LAYOUT_RECEIPT_SAVE =>
                "INSERT INTO page_layout_receipts(id,request_hash,result_json,actor_id,company_id,workspace_base,page_key,action,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            WP_QUICK_ACCESS_SAVE =>
                "INSERT INTO quick_access_preferences(user_id,surface,context_key,pinned_ids_json,version,updated_at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(user_id,surface,context_key) DO UPDATE SET pinned_ids_json=excluded.pinned_ids_json,version=excluded.version,updated_at=excluded.updated_at",
            WP_QUICK_ACCESS_RECEIPT_SAVE =>
                "INSERT INTO quick_access_preference_receipts(id,user_id,surface,context_key,request_hash,result_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
            WP_PROJECTION_RECEIPT =>
                "INSERT INTO projection_receipts(event_id,aggregate_id,aggregate_version,applied_at) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING",
            WP_WORK_ITEM_UPSERT =>
                "INSERT INTO work_items(id,source_type,source_id,source_version,title,subtitle,queue,company_id,branch_id,department_id,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(source_type,source_id) DO UPDATE SET source_version=excluded.source_version,subtitle=excluded.subtitle,queue=excluded.queue,company_id=excluded.company_id,branch_id=excluded.branch_id,department_id=excluded.department_id,updated_at=excluded.updated_at WHERE work_items.source_version<excluded.source_version",
            WP_PROJECTION_CHECKPOINT =>
                "INSERT INTO projection_checkpoints(projector,last_event_id,last_event_at,updated_at) VALUES('work-items',$1,$2,$3) ON CONFLICT(projector) DO UPDATE SET last_event_id=excluded.last_event_id,last_event_at=excluded.last_event_at,updated_at=excluded.updated_at",
            _ => return Err(BusinessError::invalid(
                "WORK_OWNER_WRITE_INVALID", "Unsupported owner write kind.")),
        })
    }
}

fn bind_one<'q>(query: BoundQuery<'q>, value: &'q Value) -> BoundQuery<'q> {
    match value {
        Value::Null => query.bind(Option::<String>::None),
        Value::Bool(value) => query.bind(*value),
        Value::Number(number) if number.is_i64() => query.bind(number.as_i64().unwrap_or(0)),
        Value::Number(number) if number.is_u64() => {
            query.bind(number.as_u64().unwrap_or(0) as i64)
        }
        Value::Number(number) => query.bind(number.as_f64().unwrap_or(0.0)),
        Value::String(value) => query.bind(value.clone()),
        other => query.bind(other.to_string()),
    }
}

fn bind_all<'q>(mut query: BoundQuery<'q>, values: &'q [Value]) -> BoundQuery<'q> {
    for value in values {
        query = bind_one(query, value);
    }
    query
}

impl OwnerWritePort for PgOwnerWrite<'_> {
    type Error = OwnerWriteError;

    async fn execute(
        &mut self,
        command: &OwnerWriteCommand<'_>,
        key: &str,
    ) -> Result<Value, Self::Error> {
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("WORK_OWNER_WRITE_TRANSACTION_REQUIRED").into());
        }
        let sql = Self::sql(command.kind).map_err(OwnerWriteError::from)?;
        let result = if returning(command.kind) {
            let row = bind_all(sqlx::query(sql), command.values)
                .fetch_optional(&mut *self.connection)
                .await?;
            row.map(|row| {
                json!({"user_id": row.try_get::<Option<String>, _>("user_id").ok().flatten()})
            })
            .unwrap_or(Value::Null)
        } else {
            bind_all(sqlx::query(sql), command.values)
                .execute(&mut *self.connection)
                .await?;
            Value::Null
        };

        let receipt_id = format!("work-owner-write:{}:{}", command.kind, key);
        let audit_id = format!("work-owner-write-audit:{}:{}", command.kind, key);
        let outbox_id = format!("work-owner-write-outbox:{}:{}", command.kind, key);
        let ledger = json!({"kind": command.kind, "companyId": command.company_id});
        sqlx::query(
            "INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(command_id) DO NOTHING",
        )
        .bind(&receipt_id)
        .bind(command.kind)
        .bind(command.actor_id)
        .bind(key)
        .bind(command.company_id)
        .bind(&ledger)
        .bind(command.now)
        .bind(json!({}))
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&audit_id)
        .bind(&receipt_id)
        .bind(command.actor_id)
        .bind("")
        .bind(command.kind)
        .bind("owner_write")
        .bind(command.company_id)
        .bind(1_i64)
        .bind("Work native owner write")
        .bind(command.now)
        .bind(json!({}))
        .execute(&mut *self.connection)
        .await?;
        sqlx::query(
            "INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) \
             VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&outbox_id)
        .bind(&receipt_id)
        .bind(format!("{}.written", command.kind))
        .bind("owner_write")
        .bind(command.company_id)
        .bind(1_i64)
        .bind(&ledger)
        .bind(command.now)
        .execute(&mut *self.connection)
        .await?;
        Ok(result)
    }
}
