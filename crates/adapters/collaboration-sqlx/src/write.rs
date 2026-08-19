use async_trait::async_trait;
use erpx_collaboration::{
    AuthorizedThreadAccess, CommitReceipt, DraftRepository, EncryptedDraft, PortError, Thread,
    ThreadCommit, ThreadWriteRepository,
};
use erpx_kernel::PrincipalId;
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Debug, Clone)]
pub struct SqlxThreadWriteRepository {
    pool: PgPool,
}

impl SqlxThreadWriteRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub const fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl ThreadWriteRepository for SqlxThreadWriteRepository {
    async fn load(&self, access: &AuthorizedThreadAccess) -> Result<Option<Thread>, PortError> {
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        let row = sqlx::query(
            "SELECT thread.anchor_record_id, thread.aggregate_version, thread.thread_state, \
                    thread.pinned, thread.created_at, thread.updated_at, \
                    policy.policy_document, anchor.anchor_document, \
                    audience.audience_document, retention.retention_document \
             FROM erpx_collaboration.threads thread \
             JOIN erpx_collaboration.thread_policies policy \
               ON policy.holding_id=thread.holding_id AND policy.thread_id=thread.thread_id \
             JOIN erpx_collaboration.thread_anchors anchor \
               ON anchor.holding_id=thread.holding_id AND anchor.thread_id=thread.thread_id \
             JOIN erpx_collaboration.thread_audiences audience \
               ON audience.holding_id=thread.holding_id AND audience.thread_id=thread.thread_id \
             JOIN erpx_collaboration.thread_retention retention \
               ON retention.holding_id=thread.holding_id AND retention.thread_id=thread.thread_id \
             WHERE thread.holding_id = $1 AND thread.thread_id = $2",
        )
        .bind(access.holding_id().as_uuid())
        .bind(access.thread_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        let Some(row) = row else {
            tx.commit().await.map_err(map_database_error)?;
            return Ok(None);
        };
        let anchor_record_id: uuid::Uuid = row
            .try_get("anchor_record_id")
            .map_err(|_| PortError::Internal)?;
        if anchor_record_id != access.anchor_record_id().as_uuid() {
            return Err(PortError::Internal);
        }
        let comments = load_comments(&mut tx, access).await?;
        let mentions = load_mentions(&mut tx, access).await?;
        let entities = load_entities(&mut tx, access).await?;
        let value = serde_json::json!({
            "id": access.thread_id().as_uuid(),
            "version": u64::try_from(row.try_get::<i64, _>("aggregate_version").map_err(|_| PortError::Internal)?)
                .map_err(|_| PortError::Internal)?,
            "policy": row.try_get::<serde_json::Value, _>("policy_document").map_err(|_| PortError::Internal)?,
            "anchor": row.try_get::<serde_json::Value, _>("anchor_document").map_err(|_| PortError::Internal)?,
            "audience": row.try_get::<serde_json::Value, _>("audience_document").map_err(|_| PortError::Internal)?,
            "state": row.try_get::<String, _>("thread_state").map_err(|_| PortError::Internal)?,
            "pinned": row.try_get::<bool, _>("pinned").map_err(|_| PortError::Internal)?,
            "comments": comments,
            "mentions": mentions,
            "reactions": entities.reactions,
            "followers": entities.followers,
            "relations": entities.relations,
            "moderation_cases": entities.moderation_cases,
            "promotions": entities.promotions,
            "action_cards": entities.action_cards,
            "retention": row.try_get::<serde_json::Value, _>("retention_document").map_err(|_| PortError::Internal)?,
            "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").map_err(|_| PortError::Internal)?,
            "updated_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("updated_at").map_err(|_| PortError::Internal)?,
        });
        let thread: Thread = serde_json::from_value(value).map_err(|_| PortError::Internal)?;
        tx.commit().await.map_err(map_database_error)?;
        thread.validate_loaded().map_err(|_| PortError::Internal)?;
        if thread.id() != access.thread_id()
            || thread.policy().holding_id() != access.holding_id()
            || thread.anchor().record().id() != access.anchor_record_id()
            || !access
                .authority_scope()
                .contains(thread.policy().visibility_scope())
        {
            return Err(PortError::Internal);
        }
        Ok(Some(thread))
    }

    async fn commit(
        &self,
        access: &AuthorizedThreadAccess,
        commit: ThreadCommit,
    ) -> Result<CommitReceipt, PortError> {
        commit.identity.validate()?;
        commit
            .thread
            .validate_loaded()
            .map_err(|_| PortError::InvalidState)?;
        validate_commit(access, &commit)?;

        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        if let Some(receipt) = replay_receipt(&mut tx, access, &commit).await? {
            tx.commit().await.map_err(map_database_error)?;
            return Ok(receipt);
        }

        let current = sqlx::query_scalar::<_, i64>(
            "SELECT aggregate_version FROM erpx_collaboration.threads \
             WHERE holding_id = $1 AND thread_id = $2 FOR UPDATE",
        )
        .bind(access.holding_id().as_uuid())
        .bind(access.thread_id().as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        match (commit.expected_version, current) {
            (None, None) => {}
            (Some(expected), Some(current)) if i64::try_from(expected).ok() == Some(current) => {}
            _ => return Err(PortError::Conflict),
        }
        let expected_next = commit
            .expected_version
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(PortError::InvalidState)?;
        if commit.thread.version() != expected_next {
            return Err(PortError::Conflict);
        }

        let aggregate_version =
            i64::try_from(commit.thread.version()).map_err(|_| PortError::InvalidState)?;
        let root_changed = if let Some(expected) = commit.expected_version {
            sqlx::query(
                "UPDATE erpx_collaboration.threads \
                 SET thread_state=$6,pinned=$7,aggregate_version=$8,updated_at=$9 \
                 WHERE holding_id=$1 AND thread_id=$2 AND anchor_record_id=$3 \
                   AND owner_context=$4 AND record_type=$5 AND aggregate_version=$10",
            )
            .bind(access.holding_id().as_uuid())
            .bind(access.thread_id().as_uuid())
            .bind(access.anchor_record_id().as_uuid())
            .bind(commit.thread.anchor().record().owner_context().slug())
            .bind(commit.thread.anchor().record().record_type().as_str())
            .bind(enum_text(commit.thread.state())?)
            .bind(commit.thread.is_pinned())
            .bind(aggregate_version)
            .bind(commit.thread.updated_at())
            .bind(i64::try_from(expected).map_err(|_| PortError::InvalidState)?)
            .execute(&mut *tx)
            .await
            .map_err(map_database_error)?
            .rows_affected()
        } else {
            sqlx::query(
                "INSERT INTO erpx_collaboration.threads \
                 (holding_id,thread_id,anchor_record_id,owner_context,record_type,classification, \
                  thread_state,pinned,aggregate_version,created_at,updated_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) \
                 ON CONFLICT (holding_id,thread_id) DO NOTHING",
            )
            .bind(access.holding_id().as_uuid())
            .bind(access.thread_id().as_uuid())
            .bind(access.anchor_record_id().as_uuid())
            .bind(commit.thread.anchor().record().owner_context().slug())
            .bind(commit.thread.anchor().record().record_type().as_str())
            .bind(format!("{:?}", commit.thread.policy().classification()).to_ascii_lowercase())
            .bind(enum_text(commit.thread.state())?)
            .bind(commit.thread.is_pinned())
            .bind(aggregate_version)
            .bind(commit.thread.created_at())
            .bind(commit.thread.updated_at())
            .execute(&mut *tx)
            .await
            .map_err(map_database_error)?
            .rows_affected()
        };
        if root_changed != 1 {
            return Err(PortError::Conflict);
        }

        persist_thread_concerns(&mut tx, access, &commit.thread).await?;

        sqlx::query(
            "INSERT INTO erpx_collaboration.thread_anchor_authority_index \
             (holding_id, thread_id, anchor_record_id, authorization_policy_id, aggregate_version) \
             VALUES ($1,$2,$3,$4,$5) \
             ON CONFLICT (holding_id, thread_id) DO UPDATE SET \
              anchor_record_id=EXCLUDED.anchor_record_id, \
              authorization_policy_id=EXCLUDED.authorization_policy_id, \
              aggregate_version=EXCLUDED.aggregate_version \
             WHERE erpx_collaboration.thread_anchor_authority_index.aggregate_version \
                   < EXCLUDED.aggregate_version",
        )
        .bind(access.holding_id().as_uuid())
        .bind(access.thread_id().as_uuid())
        .bind(access.anchor_record_id().as_uuid())
        .bind(commit.thread.policy().authorization_policy().as_uuid())
        .bind(aggregate_version)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;

        persist_comments(&mut tx, access, &commit.thread).await?;
        persist_revisions(&mut tx, access, &commit.thread).await?;
        persist_mentions(&mut tx, access, &commit.thread).await?;
        persist_entities(&mut tx, access, &commit.thread).await?;
        persist_moderation_evidence(&mut tx, access, &commit.thread).await?;

        let mut last_event_cursor = 0_u64;
        let mut stream_events = Vec::with_capacity(commit.events.len());
        for event in &commit.events {
            let payload = serde_json::to_value(event).map_err(|_| PortError::InvalidState)?;
            let cursor = sqlx::query_scalar::<_, i64>(
                "INSERT INTO erpx_collaboration.events \
                 (holding_id, thread_id, aggregate_version, event_type, event_version, payload, occurred_at, correlation_id) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING event_cursor",
            )
            .bind(access.holding_id().as_uuid())
            .bind(access.thread_id().as_uuid())
            .bind(i64::try_from(event.aggregate_version).map_err(|_| PortError::InvalidState)?)
            .bind(event_type(&event.kind))
            .bind(i32::try_from(crate::EVENT_VERSION).map_err(|_| PortError::InvalidState)?)
            .bind(payload)
            .bind(event.occurred_at)
            .bind(commit.identity.correlation_id.as_uuid())
            .fetch_one(&mut *tx)
            .await
            .map_err(map_database_error)?;
            last_event_cursor = u64::try_from(cursor).map_err(|_| PortError::Internal)?;
            stream_events.push(erpx_collaboration::CollaborationStreamEvent {
                cursor: last_event_cursor,
                holding_id: access.holding_id(),
                thread_id: access.thread_id(),
                aggregate_version: event.aggregate_version,
                kind: event.kind.clone(),
                occurred_at: event.occurred_at,
            });
        }
        if last_event_cursor == 0 {
            return Err(PortError::InvalidState);
        }

        for fact in &commit.inbox_facts {
            if fact.holding_id != access.holding_id()
                || fact.thread_id != access.thread_id()
                || fact.anchor_record_id != access.anchor_record_id()
            {
                return Err(PortError::InvalidState);
            }
            sqlx::query(
                "INSERT INTO erpx_collaboration.opaque_mention_inbox \
                 (event_id, holding_id, recipient_principal_id, thread_id, mention_event_id, anchor_record_id, created_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(fact.event_id)
            .bind(fact.holding_id.as_uuid())
            .bind(fact.recipient_principal_id.as_uuid())
            .bind(fact.thread_id.as_uuid())
            .bind(fact.mention_event_id.as_uuid())
            .bind(fact.anchor_record_id.as_uuid())
            .bind(fact.created_at)
            .execute(&mut *tx)
            .await
                .map_err(map_database_error)?;
        }

        let directory_actor = serde_json::json!({
            "principal_ref": format!("principal:{}", access.actor().id().as_uuid()),
            "kind": "holding_user",
            "session_id": null,
            "represented_principal_ref": null,
            "support_capability_id": null
        });
        crate::projection::append_projection_facts(
            &mut tx,
            access.holding_id(),
            commit.identity.command_id,
            commit.identity.correlation_id,
            directory_actor.clone(),
            &commit.thread,
            last_event_cursor,
            stream_events,
            &commit.inbox_facts,
        )
        .await?;
        for draft in crate::publication::collaboration_directory_drafts(&commit.thread)? {
            crate::publication::append_directory_draft(
                &mut tx,
                access.holding_id(),
                commit.identity.command_id,
                commit.identity.correlation_id,
                directory_actor.clone(),
                commit.thread.updated_at(),
                &draft,
            )
            .await?;
        }

        sqlx::query(
            "INSERT INTO erpx_collaboration.command_receipts \
             (holding_id, idempotency_key, command_id, request_digest, thread_id, aggregate_version, last_event_cursor, created_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,clock_timestamp())",
        )
        .bind(access.holding_id().as_uuid())
        .bind(&commit.identity.idempotency_key)
        .bind(commit.identity.command_id.as_uuid())
        .bind(commit.identity.request_digest.to_vec())
        .bind(access.thread_id().as_uuid())
        .bind(aggregate_version)
        .bind(i64::try_from(last_event_cursor).map_err(|_| PortError::InvalidState)?)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        sqlx::query(
            "INSERT INTO erpx_collaboration.audit_log \
             (holding_id,audit_id,command_id,thread_id,actor_principal_id,aggregate_version, \
              action,evidence_digest,occurred_at) \
             VALUES ($1,$2,$3,$4,$5,$6,'collaboration.thread.commit',$7,$8)",
        )
        .bind(access.holding_id().as_uuid())
        .bind(deterministic_uuid(
            commit.identity.command_id.as_uuid(),
            b"collaboration-audit-v1",
        ))
        .bind(commit.identity.command_id.as_uuid())
        .bind(access.thread_id().as_uuid())
        .bind(access.actor().id().as_uuid())
        .bind(aggregate_version)
        .bind(commit.identity.request_digest.to_vec())
        .bind(commit.thread.updated_at())
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;

        Ok(CommitReceipt {
            thread_id: access.thread_id(),
            aggregate_version: commit.thread.version(),
            last_event_cursor,
            replayed: false,
        })
    }
}

fn validate_commit(
    access: &AuthorizedThreadAccess,
    commit: &ThreadCommit,
) -> Result<(), PortError> {
    if commit.thread.id() != access.thread_id()
        || commit.thread.policy().holding_id() != access.holding_id()
        || commit.thread.anchor().record().id() != access.anchor_record_id()
        || !access
            .authority_scope()
            .contains(commit.thread.policy().visibility_scope())
        || commit.events.is_empty()
        || commit
            .events
            .last()
            .is_none_or(|event| event.aggregate_version != commit.thread.version())
    {
        return Err(PortError::InvalidState);
    }
    Ok(())
}

async fn replay_receipt(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
    commit: &ThreadCommit,
) -> Result<Option<CommitReceipt>, PortError> {
    let row = sqlx::query(
        "SELECT request_digest, thread_id, aggregate_version, last_event_cursor \
         FROM erpx_collaboration.command_receipts \
         WHERE holding_id=$1 AND idempotency_key=$2",
    )
    .bind(access.holding_id().as_uuid())
    .bind(&commit.identity.idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .map_err(map_database_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let digest: Vec<u8> = row
        .try_get("request_digest")
        .map_err(|_| PortError::Internal)?;
    let thread_id: uuid::Uuid = row.try_get("thread_id").map_err(|_| PortError::Internal)?;
    if digest != commit.identity.request_digest || thread_id != access.thread_id().as_uuid() {
        return Err(PortError::Conflict);
    }
    Ok(Some(CommitReceipt {
        thread_id: access.thread_id(),
        aggregate_version: u64::try_from(
            row.try_get::<i64, _>("aggregate_version")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        last_event_cursor: u64::try_from(
            row.try_get::<i64, _>("last_event_cursor")
                .map_err(|_| PortError::Internal)?,
        )
        .map_err(|_| PortError::Internal)?,
        replayed: true,
    }))
}

#[derive(Default)]
struct LoadedEntities {
    reactions: Vec<serde_json::Value>,
    followers: Vec<serde_json::Value>,
    relations: Vec<serde_json::Value>,
    moderation_cases: Vec<serde_json::Value>,
    promotions: Vec<serde_json::Value>,
    action_cards: Vec<serde_json::Value>,
}

async fn load_comments(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
) -> Result<Vec<serde_json::Value>, PortError> {
    let rows = sqlx::query(
        "SELECT comment_id,author_principal_id,author_kind,visibility,tombstoned_at \
         FROM erpx_collaboration.comments WHERE holding_id=$1 AND thread_id=$2 \
         ORDER BY comment_id",
    )
    .bind(access.holding_id().as_uuid())
    .bind(access.thread_id().as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(map_database_error)?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let comment_id: uuid::Uuid = row.try_get("comment_id").map_err(|_| PortError::Internal)?;
        let revisions = sqlx::query(
            "SELECT revision,author_principal_id,author_kind,document,attachments,created_at \
             FROM erpx_collaboration.comment_revisions \
             WHERE holding_id=$1 AND thread_id=$2 AND comment_id=$3 ORDER BY revision",
        )
        .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid()).bind(comment_id)
        .fetch_all(&mut **tx).await.map_err(map_database_error)?
        .into_iter()
        .map(|revision| Ok(serde_json::json!({
            "revision": u64::try_from(revision.try_get::<i64,_>("revision").map_err(|_| PortError::Internal)?).map_err(|_| PortError::Internal)?,
            "author": {"id": revision.try_get::<uuid::Uuid,_>("author_principal_id").map_err(|_| PortError::Internal)?, "kind": revision.try_get::<String,_>("author_kind").map_err(|_| PortError::Internal)?},
            "document": revision.try_get::<serde_json::Value,_>("document").map_err(|_| PortError::Internal)?,
            "attachments": revision.try_get::<serde_json::Value,_>("attachments").map_err(|_| PortError::Internal)?,
            "created_at": revision.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map_err(|_| PortError::Internal)?,
        })))
        .collect::<Result<Vec<_>, PortError>>()?;
        result.push(serde_json::json!({
            "id": comment_id,
            "author": {"id": row.try_get::<uuid::Uuid,_>("author_principal_id").map_err(|_| PortError::Internal)?, "kind": row.try_get::<String,_>("author_kind").map_err(|_| PortError::Internal)?},
            "revisions": revisions,
            "tombstoned_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>,_>("tombstoned_at").map_err(|_| PortError::Internal)?,
            "visibility": row.try_get::<String,_>("visibility").map_err(|_| PortError::Internal)?,
        }));
    }
    Ok(result)
}

async fn load_mentions(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
) -> Result<Vec<serde_json::Value>, PortError> {
    let rows = sqlx::query(
        "SELECT mention.mention_event_id,mention.comment_id,mention.revision,mention.target, \
                mention.created_at,withdrawal.withdrawn_at \
         FROM erpx_collaboration.mention_events mention \
         LEFT JOIN erpx_collaboration.mention_withdrawals withdrawal \
           ON withdrawal.holding_id=mention.holding_id \
          AND withdrawal.mention_event_id=mention.mention_event_id \
         WHERE mention.holding_id=$1 AND mention.thread_id=$2 \
         ORDER BY mention.created_at,mention.mention_event_id",
    )
    .bind(access.holding_id().as_uuid())
    .bind(access.thread_id().as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(map_database_error)?;
    rows.into_iter().map(|row| Ok(serde_json::json!({
        "id": row.try_get::<uuid::Uuid,_>("mention_event_id").map_err(|_| PortError::Internal)?,
        "comment_id": row.try_get::<uuid::Uuid,_>("comment_id").map_err(|_| PortError::Internal)?,
        "revision": u64::try_from(row.try_get::<i64,_>("revision").map_err(|_| PortError::Internal)?).map_err(|_| PortError::Internal)?,
        "target": row.try_get::<serde_json::Value,_>("target").map_err(|_| PortError::Internal)?,
        "created_at": row.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").map_err(|_| PortError::Internal)?,
        "withdrawn_at": row.try_get::<Option<chrono::DateTime<chrono::Utc>>,_>("withdrawn_at").map_err(|_| PortError::Internal)?,
    }))).collect()
}

async fn load_entities(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
) -> Result<LoadedEntities, PortError> {
    let rows = sqlx::query(
        "SELECT entity_kind,entity_document FROM erpx_collaboration.thread_entities \
         WHERE holding_id=$1 AND thread_id=$2 ORDER BY entity_kind,entity_id",
    )
    .bind(access.holding_id().as_uuid())
    .bind(access.thread_id().as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(map_database_error)?;
    let mut result = LoadedEntities::default();
    for row in rows {
        let kind: String = row
            .try_get("entity_kind")
            .map_err(|_| PortError::Internal)?;
        let value = row
            .try_get("entity_document")
            .map_err(|_| PortError::Internal)?;
        match kind.as_str() {
            "reaction" => result.reactions.push(value),
            "follower" => result.followers.push(value),
            "relation" => result.relations.push(value),
            "moderation_case" => result.moderation_cases.push(value),
            "promotion" => result.promotions.push(value),
            "action_card" => result.action_cards.push(value),
            _ => return Err(PortError::Internal),
        }
    }
    Ok(result)
}

async fn persist_thread_concerns(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
    thread: &Thread,
) -> Result<(), PortError> {
    let policy = serde_json::to_value(thread.policy()).map_err(|_| PortError::InvalidState)?;
    let anchor = serde_json::to_value(thread.anchor()).map_err(|_| PortError::InvalidState)?;
    let audience = serde_json::to_value(thread.audience()).map_err(|_| PortError::InvalidState)?;
    let retention =
        serde_json::to_value(thread.retention()).map_err(|_| PortError::InvalidState)?;
    let location = thread.anchor().location();
    let (realm, portal) = audience_realm(thread.audience());
    let policy_rows = sqlx::query("INSERT INTO erpx_collaboration.thread_policies (holding_id,thread_id,policy_document) VALUES ($1,$2,$3) ON CONFLICT (holding_id,thread_id) DO UPDATE SET policy_document=EXCLUDED.policy_document WHERE erpx_collaboration.thread_policies.policy_document=EXCLUDED.policy_document")
        .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid()).bind(policy).execute(&mut **tx).await.map_err(map_database_error)?.rows_affected();
    if policy_rows != 1 {
        return Err(PortError::Conflict);
    }
    sqlx::query("INSERT INTO erpx_collaboration.thread_anchors (holding_id,thread_id,anchor_document,source_revision,source_digest,stale) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (holding_id,thread_id) DO UPDATE SET anchor_document=EXCLUDED.anchor_document,source_revision=EXCLUDED.source_revision,source_digest=EXCLUDED.source_digest,stale=EXCLUDED.stale")
        .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid()).bind(anchor)
        .bind(location.map(|v| i64::try_from(v.source_revision)).transpose().map_err(|_| PortError::InvalidState)?)
        .bind(location.map(|v| v.source_digest.to_vec())).bind(location.is_some_and(|v| v.stale))
        .execute(&mut **tx).await.map_err(map_database_error)?;
    sqlx::query("INSERT INTO erpx_collaboration.thread_audiences (holding_id,thread_id,audience_document,audience_realm,external_portal) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (holding_id,thread_id) DO UPDATE SET audience_document=EXCLUDED.audience_document,audience_realm=EXCLUDED.audience_realm,external_portal=EXCLUDED.external_portal")
        .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid()).bind(audience).bind(realm).bind(portal)
        .execute(&mut **tx).await.map_err(map_database_error)?;
    sqlx::query("INSERT INTO erpx_collaboration.thread_retention (holding_id,thread_id,retention_document,parent_retain_until,comment_retain_until,jurisdiction_retain_until) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (holding_id,thread_id) DO UPDATE SET retention_document=EXCLUDED.retention_document,parent_retain_until=EXCLUDED.parent_retain_until,comment_retain_until=EXCLUDED.comment_retain_until,jurisdiction_retain_until=EXCLUDED.jurisdiction_retain_until")
        .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid()).bind(retention)
        .bind(thread.retention().parent_retain_until).bind(thread.retention().comment_retain_until).bind(thread.retention().jurisdiction_retain_until)
        .execute(&mut **tx).await.map_err(map_database_error)?;
    Ok(())
}

async fn persist_comments(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
    thread: &Thread,
) -> Result<(), PortError> {
    for comment in thread.comments() {
        let rows = sqlx::query("INSERT INTO erpx_collaboration.comments (holding_id,thread_id,comment_id,author_principal_id,author_kind,visibility,tombstoned_at) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (holding_id,thread_id,comment_id) DO UPDATE SET visibility=EXCLUDED.visibility,tombstoned_at=EXCLUDED.tombstoned_at WHERE erpx_collaboration.comments.author_principal_id=EXCLUDED.author_principal_id AND erpx_collaboration.comments.author_kind=EXCLUDED.author_kind")
            .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid()).bind(comment.id().as_uuid())
            .bind(comment.author().id().as_uuid()).bind(enum_text(comment.author().kind())?).bind(enum_text(comment.visibility())?)
            .bind(comment.tombstoned_at()).execute(&mut **tx).await.map_err(map_database_error)?.rows_affected();
        if rows != 1 {
            return Err(PortError::Conflict);
        }
    }
    Ok(())
}

async fn persist_entities(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
    thread: &Thread,
) -> Result<(), PortError> {
    sqlx::query(
        "DELETE FROM erpx_collaboration.thread_entities WHERE holding_id=$1 AND thread_id=$2",
    )
    .bind(access.holding_id().as_uuid())
    .bind(access.thread_id().as_uuid())
    .execute(&mut **tx)
    .await
    .map_err(map_database_error)?;
    let mut entities = Vec::new();
    entities.extend(
        thread
            .reactions()
            .iter()
            .enumerate()
            .map(|(i, v)| ("reaction", format!("{i:08}"), serde_json::to_value(v))),
    );
    entities.extend(thread.followers().iter().map(|v| {
        (
            "follower",
            v.principal.id().to_string(),
            serde_json::to_value(v),
        )
    }));
    entities.extend(thread.relations().iter().map(|v| {
        (
            "relation",
            v.id.as_uuid().to_string(),
            serde_json::to_value(v),
        )
    }));
    entities.extend(thread.moderation_cases().iter().map(|v| {
        (
            "moderation_case",
            v.id.as_uuid().to_string(),
            serde_json::to_value(v),
        )
    }));
    entities.extend(thread.promotions().iter().map(|v| {
        (
            "promotion",
            v.id.as_uuid().to_string(),
            serde_json::to_value(v),
        )
    }));
    entities.extend(thread.action_cards().iter().map(|v| {
        (
            "action_card",
            v.id.as_uuid().to_string(),
            serde_json::to_value(v),
        )
    }));
    for (kind, id, value) in entities {
        sqlx::query("INSERT INTO erpx_collaboration.thread_entities (holding_id,thread_id,entity_kind,entity_id,entity_document,updated_at) VALUES ($1,$2,$3,$4,$5,$6)")
            .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid()).bind(kind).bind(id)
            .bind(value.map_err(|_| PortError::InvalidState)?).bind(thread.updated_at()).execute(&mut **tx).await.map_err(map_database_error)?;
    }
    Ok(())
}

fn audience_realm(audience: &erpx_collaboration::Audience) -> (&'static str, Option<&'static str>) {
    use erpx_collaboration::{Audience, ExternalPortal};
    match audience {
        Audience::Internal { .. } => ("internal", None),
        Audience::ExternalPortal { portal, .. } => (
            "external_portal",
            Some(match portal {
                ExternalPortal::Customer => "customer",
                ExternalPortal::Supplier => "supplier",
                ExternalPortal::Candidate => "candidate",
                ExternalPortal::Commerce => "commerce",
                ExternalPortal::Signatory => "signatory",
            }),
        ),
    }
}

fn enum_text<T: serde::Serialize>(value: T) -> Result<String, PortError> {
    serde_json::to_value(value)
        .map_err(|_| PortError::InvalidState)?
        .as_str()
        .map(str::to_owned)
        .ok_or(PortError::InvalidState)
}

fn deterministic_uuid(seed: uuid::Uuid, label: &[u8]) -> uuid::Uuid {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes)
}

async fn persist_revisions(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
    thread: &Thread,
) -> Result<(), PortError> {
    for comment in thread.comments() {
        for revision in comment.revisions() {
            let document =
                serde_json::to_value(revision.document()).map_err(|_| PortError::InvalidState)?;
            let attachments = serde_json::to_value(revision.attachments())
                .map_err(|_| PortError::InvalidState)?;
            let author_kind = enum_text(revision.author().kind())?;
            let revision_number =
                i64::try_from(revision.revision()).map_err(|_| PortError::InvalidState)?;
            let inserted = sqlx::query(
                "INSERT INTO erpx_collaboration.comment_revisions \
                 (holding_id, thread_id, comment_id, revision, author_principal_id, author_kind, document, plain_text, content_digest, attachments, created_at) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT DO NOTHING",
            )
            .bind(access.holding_id().as_uuid())
            .bind(access.thread_id().as_uuid())
            .bind(comment.id().as_uuid())
            .bind(revision_number)
            .bind(revision.author().id().as_uuid())
            .bind(&author_kind)
            .bind(&document)
            .bind(revision.document().plain_text())
            .bind(revision.document().digest().to_vec())
            .bind(&attachments)
            .bind(revision.created_at())
            .execute(&mut **tx)
            .await
            .map_err(map_database_error)?
            .rows_affected();
            if inserted == 0 {
                let exact: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM erpx_collaboration.comment_revisions \
                     WHERE holding_id=$1 AND thread_id=$2 AND comment_id=$3 AND revision=$4 \
                       AND author_principal_id=$5 AND author_kind=$6 AND document=$7 \
                       AND plain_text=$8 AND content_digest=$9 AND attachments=$10 AND created_at=$11)",
                )
                .bind(access.holding_id().as_uuid()).bind(access.thread_id().as_uuid())
                .bind(comment.id().as_uuid()).bind(revision_number)
                .bind(revision.author().id().as_uuid()).bind(&author_kind).bind(&document)
                .bind(revision.document().plain_text()).bind(revision.document().digest().to_vec())
                .bind(&attachments).bind(revision.created_at()).fetch_one(&mut **tx).await
                .map_err(map_database_error)?;
                if !exact {
                    return Err(PortError::Conflict);
                }
            }
        }
    }
    Ok(())
}

async fn persist_mentions(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
    thread: &Thread,
) -> Result<(), PortError> {
    for mention in thread.mentions() {
        let target = serde_json::to_value(mention.target()).map_err(|_| PortError::InvalidState)?;
        let revision = i64::try_from(mention.revision()).map_err(|_| PortError::InvalidState)?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_collaboration.mention_events \
             (holding_id, thread_id, mention_event_id, comment_id, revision, target, created_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
        )
        .bind(access.holding_id().as_uuid())
        .bind(access.thread_id().as_uuid())
        .bind(mention.id().as_uuid())
        .bind(mention.comment_id().as_uuid())
        .bind(revision)
        .bind(&target)
        .bind(mention.created_at())
        .execute(&mut **tx)
        .await
        .map_err(map_database_error)?
        .rows_affected();
        if inserted == 0 {
            let exact: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM erpx_collaboration.mention_events \
                 WHERE holding_id=$1 AND thread_id=$2 AND mention_event_id=$3 \
                   AND comment_id=$4 AND revision=$5 AND target=$6 AND created_at=$7)",
            )
            .bind(access.holding_id().as_uuid())
            .bind(access.thread_id().as_uuid())
            .bind(mention.id().as_uuid())
            .bind(mention.comment_id().as_uuid())
            .bind(revision)
            .bind(&target)
            .bind(mention.created_at())
            .fetch_one(&mut **tx)
            .await
            .map_err(map_database_error)?;
            if !exact {
                return Err(PortError::Conflict);
            }
        }
        if let Some(withdrawn_at) = mention.withdrawn_at() {
            let inserted = sqlx::query(
                "INSERT INTO erpx_collaboration.mention_withdrawals \
                 (holding_id, mention_event_id, withdrawn_at) VALUES ($1,$2,$3) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(access.holding_id().as_uuid())
            .bind(mention.id().as_uuid())
            .bind(withdrawn_at)
            .execute(&mut **tx)
            .await
            .map_err(map_database_error)?
            .rows_affected();
            if inserted == 0 {
                let exact: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM erpx_collaboration.mention_withdrawals \
                     WHERE holding_id=$1 AND mention_event_id=$2 AND withdrawn_at=$3)",
                )
                .bind(access.holding_id().as_uuid())
                .bind(mention.id().as_uuid())
                .bind(withdrawn_at)
                .fetch_one(&mut **tx)
                .await
                .map_err(map_database_error)?;
                if !exact {
                    return Err(PortError::Conflict);
                }
            }
        }
    }
    Ok(())
}

async fn persist_moderation_evidence(
    tx: &mut Transaction<'_, Postgres>,
    access: &AuthorizedThreadAccess,
    thread: &Thread,
) -> Result<(), PortError> {
    for case in thread.moderation_cases() {
        let key_version =
            i32::try_from(case.evidence.key_version).map_err(|_| PortError::InvalidState)?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_collaboration.moderation_evidence \
             (holding_id, case_id, thread_id, encrypted_object_id, evidence_digest, key_version, created_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
        )
        .bind(access.holding_id().as_uuid())
        .bind(case.id.as_uuid())
        .bind(access.thread_id().as_uuid())
        .bind(case.evidence.encrypted_object_id)
        .bind(case.evidence.digest.to_vec())
        .bind(key_version)
        .bind(case.created_at)
        .execute(&mut **tx)
        .await
        .map_err(map_database_error)?
        .rows_affected();
        if inserted == 0 {
            let exact: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM erpx_collaboration.moderation_evidence \
                 WHERE holding_id=$1 AND case_id=$2 AND thread_id=$3 \
                   AND encrypted_object_id=$4 AND evidence_digest=$5 AND key_version=$6 AND created_at=$7)",
            )
            .bind(access.holding_id().as_uuid()).bind(case.id.as_uuid())
            .bind(access.thread_id().as_uuid()).bind(case.evidence.encrypted_object_id)
            .bind(case.evidence.digest.to_vec()).bind(key_version).bind(case.created_at)
            .fetch_one(&mut **tx).await.map_err(map_database_error)?;
            if !exact {
                return Err(PortError::Conflict);
            }
        }
    }
    Ok(())
}

fn event_type(value: &erpx_collaboration::CollaborationEventKind) -> &'static str {
    use erpx_collaboration::CollaborationEventKind as Event;
    match value {
        Event::ThreadCreated => "collaboration.thread_created",
        Event::CommentAdded { .. } => "collaboration.comment_added",
        Event::CommentRevised { .. } => "collaboration.comment_revised",
        Event::CommentTombstoned { .. } => "collaboration.comment_tombstoned",
        Event::MentionCreated { .. } => "collaboration.mention_created",
        Event::MentionWithdrawn { .. } => "collaboration.mention_withdrawn",
        Event::ThreadResolved => "collaboration.thread_resolved",
        Event::ThreadReopened => "collaboration.thread_reopened",
        Event::PinChanged { .. } => "collaboration.pin_changed",
        Event::ReactionChanged { .. } => "collaboration.reaction_changed",
        Event::FollowingChanged { .. } => "collaboration.following_changed",
        Event::RelationChanged { .. } => "collaboration.relation_changed",
        Event::ModerationChanged { .. } => "collaboration.moderation_changed",
        Event::PromotionProposed { .. } => "collaboration.promotion_proposed",
        Event::ActionCardChanged { .. } => "collaboration.action_card_changed",
        Event::AudienceNarrowed => "collaboration.audience_narrowed",
        Event::AnchorMarkedStale => "collaboration.anchor_marked_stale",
    }
}

#[derive(Debug, Clone)]
pub struct SqlxDraftRepository {
    pool: PgPool,
}

impl SqlxDraftRepository {
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DraftRepository for SqlxDraftRepository {
    async fn save_encrypted(
        &self,
        access: &AuthorizedThreadAccess,
        draft: &EncryptedDraft,
    ) -> Result<(), PortError> {
        if draft.thread_id != access.thread_id() || draft.owner != access.actor().id() {
            return Err(PortError::Denied);
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        sqlx::query(
            "INSERT INTO erpx_collaboration.encrypted_drafts \
             (holding_id, draft_id, thread_id, owner_principal_id, ciphertext, key_version, expires_at, updated_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) \
             ON CONFLICT (holding_id, thread_id, owner_principal_id) DO UPDATE SET \
              draft_id=EXCLUDED.draft_id, ciphertext=EXCLUDED.ciphertext, key_version=EXCLUDED.key_version, \
              expires_at=EXCLUDED.expires_at, updated_at=EXCLUDED.updated_at",
        )
        .bind(access.holding_id().as_uuid())
        .bind(draft.id.as_uuid())
        .bind(draft.thread_id.as_uuid())
        .bind(draft.owner.as_uuid())
        .bind(&draft.ciphertext)
        .bind(i32::try_from(draft.key_version).map_err(|_| PortError::InvalidState)?)
        .bind(draft.expires_at)
        .bind(draft.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }

    async fn load_encrypted(
        &self,
        access: &AuthorizedThreadAccess,
        owner: PrincipalId,
    ) -> Result<Option<EncryptedDraft>, PortError> {
        if owner != access.actor().id() {
            return Err(PortError::Denied);
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        let row = sqlx::query(
            "SELECT draft_id, thread_id, owner_principal_id, ciphertext, key_version, \
                    expires_at, updated_at \
             FROM erpx_collaboration.encrypted_drafts \
             WHERE holding_id=$1 AND thread_id=$2 AND owner_principal_id=$3 AND expires_at>clock_timestamp()",
        )
        .bind(access.holding_id().as_uuid())
        .bind(access.thread_id().as_uuid())
        .bind(owner.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(EncryptedDraft {
            id: erpx_collaboration::DraftId::try_from_uuid(
                row.try_get("draft_id").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            thread_id: erpx_collaboration::ThreadId::try_from_uuid(
                row.try_get("thread_id").map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            owner: PrincipalId::try_from_uuid(
                row.try_get("owner_principal_id")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            ciphertext: row.try_get("ciphertext").map_err(|_| PortError::Internal)?,
            key_version: u32::try_from(
                row.try_get::<i32, _>("key_version")
                    .map_err(|_| PortError::Internal)?,
            )
            .map_err(|_| PortError::Internal)?,
            expires_at: row.try_get("expires_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        }))
    }

    async fn delete(
        &self,
        access: &AuthorizedThreadAccess,
        owner: PrincipalId,
    ) -> Result<(), PortError> {
        if owner != access.actor().id() {
            return Err(PortError::Denied);
        }
        let mut tx = self.pool.begin().await.map_err(map_database_error)?;
        set_holding(&mut tx, access.holding_id()).await?;
        sqlx::query(
            "DELETE FROM erpx_collaboration.encrypted_drafts \
             WHERE holding_id=$1 AND thread_id=$2 AND owner_principal_id=$3",
        )
        .bind(access.holding_id().as_uuid())
        .bind(access.thread_id().as_uuid())
        .bind(owner.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_database_error)?;
        tx.commit().await.map_err(map_database_error)?;
        Ok(())
    }
}

async fn set_holding(
    tx: &mut Transaction<'_, Postgres>,
    holding_id: erpx_kernel::HoldingId,
) -> Result<(), PortError> {
    let expected = holding_id.to_string();
    let bound = sqlx::query_scalar::<_, String>("SELECT set_config('app.holding_id', $1, true)")
        .bind(holding_id.to_string())
        .fetch_one(&mut **tx)
        .await
        .map_err(map_database_error)?;
    (bound == expected).then_some(()).ok_or(PortError::Denied)
}

fn map_database_error(value: sqlx::Error) -> PortError {
    match value.as_database_error().and_then(|error| error.code()) {
        Some(code) if code == "23505" || code == "40001" => PortError::Conflict,
        _ => PortError::Unavailable,
    }
}
