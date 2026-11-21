use crate::{
    application::canonical_original::OriginalByteStore,
    domain::canonical_original::{
        CanonicalOriginalFacts, LegalOriginalQuery, OriginalReadActor, changed, unavailable,
    },
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, HoldingId, IdentifierSource, RecordId,
};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use std::future::Future;

type Error = PgTransactionError<BusinessError>;

/// Composition supplies the complete retained Identity/Commercial/Legal parent.
/// No implementation grants access by default. prepare acquires business owner
/// locks before shared Files locks; authorize checks current permission, purchase,
/// audience, preservation and the exact immutable Legal document/link binding.
pub trait PgOriginalReadPolicy {
    fn prepare(
        &mut self,
        connection: &mut PgConnection,
        actor: &OriginalReadActor,
        query: &LegalOriginalQuery,
    ) -> impl Future<Output = Result<(), Error>> + Send;
    fn authorize(
        &mut self,
        connection: &mut PgConnection,
        actor: &OriginalReadActor,
        query: &LegalOriginalQuery,
        facts: &CanonicalOriginalFacts,
    ) -> impl Future<Output = Result<String, Error>> + Send;
}

/// Opaque, non-deserializable evidence tied to the exact caller transaction and
/// full source/authority snapshot. A string from an HTTP request cannot mint it.
#[derive(Clone)]
pub struct OriginalReadWitness {
    query: LegalOriginalQuery,
    actor: OriginalReadActor,
    transaction: (String, i32, String),
    fingerprint: String,
}
impl OriginalReadWitness {
    pub fn as_str(&self) -> &str {
        &self.fingerprint
    }
}

pub struct OriginalRead {
    pub bytes: Vec<u8>,
    pub witness: OriginalReadWitness,
    pub input_identity: OriginalInputIdentity,
}

/// Computation input data only: intentionally constructible, not a source
/// lease, permission or transaction proof. Compare it with a newly authorized
/// canonical read after compute; it can never replace that read's witness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OriginalInputIdentity {
    pub source_data_sha256: String,
}

pub struct PgCanonicalOriginalReader<'c, Policy, Store, ClockSource, Identifiers> {
    holding_id: HoldingId,
    company_id: CompanyId,
    connection: &'c mut PgConnection,
    policy: &'c mut Policy,
    store: &'c Store,
    clock: &'c ClockSource,
    identifiers: &'c mut Identifiers,
}

impl<
    'c,
    Policy: PgOriginalReadPolicy,
    Store: OriginalByteStore,
    ClockSource: Clock,
    Identifiers: IdentifierSource,
> PgCanonicalOriginalReader<'c, Policy, Store, ClockSource, Identifiers>
{
    pub fn new(
        holding_id: HoldingId,
        company_id: CompanyId,
        connection: &'c mut PgConnection,
        policy: &'c mut Policy,
        store: &'c Store,
        clock: &'c ClockSource,
        identifiers: &'c mut Identifiers,
    ) -> Self {
        Self {
            holding_id,
            company_id,
            connection,
            policy,
            store,
            clock,
            identifiers,
        }
    }

    pub async fn read(
        &mut self,
        actor: &OriginalReadActor,
        query: &LegalOriginalQuery,
    ) -> Result<OriginalRead, Error> {
        query.validate(actor)?;
        if query.holding_id != self.holding_id || query.company_id != self.company_id {
            return Err(unavailable().into());
        }
        lock_canonical_original_sources(self.connection).await?;
        self.policy.prepare(self.connection, actor, query).await?;
        let transaction = self.transaction(actor, query).await?;
        let (facts, source) = self.source(query).await?;
        let authority = self.authority(actor, query, &facts).await?;
        let bytes =
            crate::application::canonical_original::read_original_bytes(self.store, &facts, query)
                .await?;
        let scoped_fingerprint = fingerprint(&source, &authority, actor, query, &transaction);
        let input_identity = OriginalInputIdentity {
            source_data_sha256: source_data_digest(&source, query),
        };
        let witness = OriginalReadWitness {
            query: query.clone(),
            actor: actor.clone(),
            transaction,
            fingerprint: scoped_fingerprint,
        };
        self.revalidate(actor, query, &witness).await?;
        sqlx::query("INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,'original','allowed',$6)")
            .bind(self.identifiers.next().as_str()).bind(query.company_id.as_str()).bind(facts.source_id.as_str()).bind(query.link_id.as_str()).bind(actor.user_id.as_str()).bind(self.clock.now().get()).execute(&mut *self.connection).await?;
        Ok(OriginalRead {
            bytes,
            witness,
            input_identity,
        })
    }

    pub async fn revalidate(
        &mut self,
        actor: &OriginalReadActor,
        query: &LegalOriginalQuery,
        witness: &OriginalReadWitness,
    ) -> Result<(), Error> {
        if witness.actor != *actor
            || witness.query != *query
            || witness.transaction != self.transaction(actor, query).await?
        {
            return Err(changed().into());
        }
        let (facts, source) = self.source(query).await?;
        let authority = self.authority(actor, query, &facts).await?;
        if witness.fingerprint
            != fingerprint(&source, &authority, actor, query, &witness.transaction)
        {
            return Err(changed().into());
        }
        Ok(())
    }

    async fn transaction(
        &mut self,
        actor: &OriginalReadActor,
        query: &LegalOriginalQuery,
    ) -> Result<(String, i32, String), Error> {
        query.validate(actor)?;
        if query.holding_id != self.holding_id || query.company_id != self.company_id {
            return Err(unavailable().into());
        }
        if !self.connection.is_in_transaction() {
            return Err(PersistenceError::new("FILE_ORIGINAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("FILE_ORIGINAL_SERIALIZABLE_REQUIRED").into());
        }
        let row = sqlx::query("SELECT current_database() AS db,pg_backend_pid() AS pid,pg_current_xact_id()::text AS tx").fetch_one(&mut *self.connection).await?;
        Ok((row.try_get("db")?, row.try_get("pid")?, row.try_get("tx")?))
    }

    async fn authority(
        &mut self,
        actor: &OriginalReadActor,
        query: &LegalOriginalQuery,
        facts: &CanonicalOriginalFacts,
    ) -> Result<String, Error> {
        let authority = self
            .policy
            .authorize(self.connection, actor, query, facts)
            .await?;
        if authority.is_empty() || authority.len() > 200 {
            return Err(changed().into());
        }
        Ok(authority)
    }

    async fn source(
        &mut self,
        query: &LegalOriginalQuery,
    ) -> Result<(CanonicalOriginalFacts, String), Error> {
        let row = sqlx::query("SELECT s.company_id,s.id AS set_id,s.target_type,s.target_id,s.purpose_key,s.component_kind,s.component_ref,s.policy_version,s.classification,s.audience_json,s.retention_policy_key,s.state AS set_state,l.id AS link_id,l.state AS link_state,l.binding_type,l.binding_id,a.id AS source_id,a.company_id AS source_company_id,a.owner_type,a.owner_id,a.purpose,a.object_key,a.media_type,a.detected_media_type,a.bytes,a.checksum_sha256,a.source_state,a.revoked_at,a.destroyed_at,a.scanned_at,a.scanner_definition_at, jsonb_build_object('link',to_jsonb(l),'set',to_jsonb(s),'source',to_jsonb(a),'intent',to_jsonb(u),'scans',(SELECT jsonb_agg(to_jsonb(x) ORDER BY x.id) FROM attachment_scan_attempts x WHERE x.attachment_id=a.id))::text AS source_image FROM attachment_links l JOIN attachment_sets s ON s.id=l.set_id JOIN attachments a ON a.id=l.attachment_id AND a.company_id=s.company_id LEFT JOIN upload_intents u ON u.id=coalesce(l.upload_intent_id,a.upload_id) AND u.company_id=s.company_id WHERE l.id=$1 AND s.company_id=$2 FOR SHARE OF l,s,a")
            .bind(query.link_id.as_str()).bind(query.company_id.as_str()).fetch_optional(&mut *self.connection).await?.ok_or_else(unavailable)?;
        // A table lock prevents later writers but cannot refresh an existing
        // SERIALIZABLE snapshot. Tuple locks make an earlier committed source
        // update raise 40001, preserving replay at the outer Holding boundary.
        sqlx::query("SELECT id FROM upload_intents WHERE id=(SELECT coalesce(l.upload_intent_id,a.upload_id) FROM attachment_links l JOIN attachments a ON a.id=l.attachment_id WHERE l.id=$1) FOR SHARE")
            .bind(query.link_id.as_str()).execute(&mut *self.connection).await?;
        sqlx::query("SELECT id FROM attachment_scan_attempts WHERE attachment_id=$1 FOR SHARE")
            .bind(row.try_get::<String, _>("source_id")?)
            .execute(&mut *self.connection)
            .await?;
        let id = |name| -> Result<RecordId, Error> {
            RecordId::new(row.try_get::<String, _>(name)?).map_err(Error::from)
        };
        let facts = CanonicalOriginalFacts {
            company_id: CompanyId::new(row.try_get::<String, _>("company_id")?)?,
            link_id: id("link_id")?,
            source_id: id("source_id")?,
            set_id: id("set_id")?,
            target_type: row.try_get("target_type")?,
            target_id: id("target_id")?,
            purpose_key: row.try_get("purpose_key")?,
            component_kind: row.try_get("component_kind")?,
            component_ref: row.try_get("component_ref")?,
            policy_version: row.try_get("policy_version")?,
            classification: row.try_get("classification")?,
            audience_json: row.try_get("audience_json")?,
            retention_policy_key: row.try_get("retention_policy_key")?,
            set_state: row.try_get("set_state")?,
            link_state: row.try_get("link_state")?,
            binding_type: row.try_get("binding_type")?,
            binding_id: row.try_get("binding_id")?,
            source_state: row.try_get("source_state")?,
            source_company_id: row.try_get("source_company_id")?,
            source_owner_type: row.try_get("owner_type")?,
            source_owner_id: row.try_get("owner_id")?,
            source_purpose: row.try_get("purpose")?,
            object_key: row.try_get("object_key")?,
            media_type: row.try_get("media_type")?,
            detected_media_type: row.try_get("detected_media_type")?,
            bytes: row.try_get("bytes")?,
            checksum_sha256: row.try_get("checksum_sha256")?,
            revoked_at: row.try_get("revoked_at")?,
            destroyed_at: row.try_get("destroyed_at")?,
            scanned_at: row.try_get("scanned_at")?,
            scanner_definition_at: row.try_get("scanner_definition_at")?,
        };
        facts.validate(query)?;
        Ok((facts, row.try_get("source_image")?))
    }
}
fn fingerprint(
    source: &str,
    authority: &str,
    actor: &OriginalReadActor,
    query: &LegalOriginalQuery,
    transaction: &(String, i32, String),
) -> String {
    let mut hash = Sha256::new();
    let backend = transaction.1.to_string();
    let limit = query.max_bytes.to_string();
    for part in [
        source,
        authority,
        actor.holding_id.as_str(),
        actor.company_id.as_str(),
        actor.user_id.as_str(),
        actor.selection_revision.as_str(),
        query.item_id.as_str(),
        query.document_id.as_str(),
        query.link_id.as_str(),
        query.expected_checksum_sha256.as_str(),
        query.expected_media_type.as_mime(),
        limit.as_str(),
        transaction.0.as_str(),
        backend.as_str(),
        transaction.2.as_str(),
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    hex::encode(hash.finalize())
}

fn source_data_digest(source: &str, query: &LegalOriginalQuery) -> String {
    let mut hash = Sha256::new();
    for part in [
        "files-original-input/v1",
        query.holding_id.as_str(),
        query.company_id.as_str(),
        query.item_id.as_str(),
        query.document_id.as_str(),
        query.link_id.as_str(),
        query.expected_media_type.as_mime(),
        query.expected_checksum_sha256.as_str(),
        source,
    ] {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part);
    }
    hex::encode(hash.finalize())
}

/// Utility-only source lease for a fresh coordinating Holding transaction.
/// Acquire every participating owner lease before its first SELECT. This is not
/// an original-read witness or a way to refresh an already-established snapshot.
pub async fn lock_canonical_original_sources(connection: &mut PgConnection) -> Result<(), Error> {
    if !connection.is_in_transaction() {
        return Err(PersistenceError::new("FILE_ORIGINAL_OWNER_TRANSACTION_REQUIRED").into());
    }
    // Match the existing reader. Locks persist through caller audit/receipt.
    sqlx::query("LOCK TABLE attachment_links,attachment_sets,attachments,upload_intents,attachment_scan_attempts IN SHARE ROW EXCLUSIVE MODE")
        .execute(connection).await?;
    Ok(())
}
