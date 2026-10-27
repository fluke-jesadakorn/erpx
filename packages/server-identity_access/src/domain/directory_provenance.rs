//! Durable Directory source data is not membership, an Actor or permission.
const FRESH_MS: i64 = super::directory::DIRECTORY_FRESH_MS;
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ProfileTag(pub(crate) [u8; 32]);
impl ProfileTag {
    pub(crate) fn from_bytes(value: &[u8]) -> Result<Self, Denial> {
        Ok(Self(
            value
                .try_into()
                .map_err(|_| Denial::InconsistentObservation)?,
        ))
    }
    pub fn database_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
pub(crate) fn valid_key_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Denial {
    InvalidKeyRing,
    MissingKey,
    ProfileUnavailable,
    SourceNotApproved,
    LocalSourceMismatch,
    InvalidRevision,
    RevisionConflict,
    ConfigurationNotSelected,
    InconsistentObservation,
    InvalidClock,
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct SourceRevision(i64);
impl SourceRevision {
    pub fn checked(value: i64) -> Result<Self, Denial> {
        if value <= 0 {
            return Err(Denial::InvalidRevision);
        }
        Ok(Self(value))
    }
    pub fn get(self) -> i64 {
        self.0
    }
    fn next(self) -> Result<Self, Denial> {
        Self::checked(self.0.checked_add(1).ok_or(Denial::InvalidRevision)?)
    }
}
#[derive(Clone, Eq, PartialEq)]
pub struct SelectedSource {
    pub revision: SourceRevision,
    pub key_id: String,
    pub profile_tag: ProfileTag,
    pub connection_version: i64,
    pub config_fingerprint: [u8; 32],
}
pub struct CurrentLocalSource {
    pub key_id: String,
    pub profile_tag: ProfileTag,
    pub connection_version: i64,
    pub config_fingerprint: [u8; 32],
}
#[derive(Clone)]
pub struct ObservationProvenance {
    pub source: SelectedSource,
    pub observed_at: i64,
}
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum ObservationUse {
    Fresh,
    CompleteRefreshRequired,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum SnapshotOrder {
    ApplyComplete,
    RetainNewerFromSameSource,
}
/// Existing last_sync_at alone must never skip a complete observation from a
/// newly approved revision. An earlier source is not a newer current proof.
pub fn snapshot_order(
    current: &SelectedSource,
    previous: Option<&ObservationProvenance>,
    incoming_observed_at: i64,
    now: i64,
) -> Result<SnapshotOrder, Denial> {
    if incoming_observed_at < 0
        || incoming_observed_at > now
        || incoming_observed_at <= now.saturating_sub(FRESH_MS)
    {
        return Err(Denial::InvalidClock);
    }
    if let Some(previous) = previous {
        if previous.source.revision == current.revision && previous.source != *current {
            return Err(Denial::InconsistentObservation);
        }
        if previous.source == *current && previous.observed_at > incoming_observed_at {
            if previous.observed_at > now {
                return Err(Denial::InvalidClock);
            }
            return Ok(SnapshotOrder::RetainNewerFromSameSource);
        }
    }
    Ok(SnapshotOrder::ApplyComplete)
}

/// A missing source is a configuration failure. It is never synthesized from
/// whichever profile an API or worker happened to load first.
pub fn classify(
    approved: Option<&SelectedSource>,
    local: &CurrentLocalSource,
    observation: Option<&ObservationProvenance>,
    identity_observed_at: i64,
    now: i64,
) -> Result<ObservationUse, Denial> {
    let source = approved.ok_or(Denial::SourceNotApproved)?;
    if source.connection_version != local.connection_version
        || source.config_fingerprint != local.config_fingerprint
    {
        return Err(Denial::ConfigurationNotSelected);
    }
    if source.key_id != local.key_id || source.profile_tag != local.profile_tag {
        return Err(Denial::LocalSourceMismatch);
    }
    observation_status(source, observation, identity_observed_at, now)
}

pub fn observation_status(
    source: &SelectedSource,
    observation: Option<&ObservationProvenance>,
    identity_observed_at: i64,
    now: i64,
) -> Result<ObservationUse, Denial> {
    if now < 0 {
        return Err(Denial::InvalidClock);
    }
    let Some(observation) = observation else {
        return Ok(ObservationUse::CompleteRefreshRequired);
    };
    if observation.source.revision != source.revision {
        return Ok(ObservationUse::CompleteRefreshRequired);
    }
    if observation.source != *source {
        return Err(Denial::InconsistentObservation);
    }
    if observation.observed_at < 0 || observation.observed_at > now || identity_observed_at > now {
        return Err(Denial::InvalidClock);
    }
    if identity_observed_at != observation.observed_at
        || observation.observed_at <= now.saturating_sub(FRESH_MS)
    {
        return Ok(ObservationUse::CompleteRefreshRequired);
    }
    Ok(ObservationUse::Fresh)
}

/// Only the future reviewed native admin command may call this under the
/// selected-source row lock. A seed also requires an explicit expected absence.
/// This pure transition does not perform authorization or database writes.
pub fn rotate(
    current: &SelectedSource,
    expected_revision: SourceRevision,
    candidate: CurrentLocalSource,
) -> Result<SelectedSource, Denial> {
    if current.revision != expected_revision {
        return Err(Denial::RevisionConflict);
    }
    if !valid_key_id(&candidate.key_id) || candidate.connection_version <= 0 {
        return Err(Denial::ConfigurationNotSelected);
    }
    Ok(SelectedSource {
        revision: current.revision.next()?,
        key_id: candidate.key_id,
        profile_tag: candidate.profile_tag,
        connection_version: candidate.connection_version,
        config_fingerprint: candidate.config_fingerprint,
    })
}

impl Denial {
    pub fn business(self) -> erpx_server_shared::public::BusinessError {
        erpx_server_shared::public::BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "DIRECTORY_SOURCE_CONFIGURATION",
            "The approved Directory source or its current observation is unavailable.",
        )
    }
}
