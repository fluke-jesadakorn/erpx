//! Quick-access preference use cases on a caller-owned Control transaction.
use crate::domain::quick_access_preferences::{
    QuickAccessPreference, SaveQuickAccessInput, idempotency_conflict, shortcut_unavailable,
    validate, version_conflict,
};
use erpx_server_shared::public::BusinessError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuickAccessReceipt {
    pub user_id: String,
    pub surface: String,
    pub context_key: String,
    pub request_hash: String,
    pub result_json: String,
}

pub trait QuickAccessRepository {
    type Error: From<BusinessError> + Send + 'static;

    fn read<'a>(
        &'a mut self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
    ) -> impl Future<Output = Result<Option<QuickAccessPreference>, Self::Error>> + Send + 'a;

    fn receipt<'a>(
        &'a mut self,
        request_key: &'a str,
    ) -> impl Future<Output = Result<Option<QuickAccessReceipt>, Self::Error>> + Send + 'a;

    fn save_preference<'a>(
        &'a mut self,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
        result: &'a QuickAccessPreference,
        now: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;

    fn save_receipt<'a>(
        &'a mut self,
        request_key: &'a str,
        user_id: &'a str,
        surface: &'a str,
        context_key: &'a str,
        request_hash: &'a str,
        result: &'a QuickAccessPreference,
        now: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'a;
}

#[derive(Serialize)]
struct HashPayload<'a> {
    #[serde(rename = "pinnedIds")]
    pinned_ids: Option<&'a Vec<String>>,
    #[serde(rename = "expectedVersion")]
    expected_version: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreferenceWire {
    pinned_ids: Option<Vec<String>>,
    version: i64,
}

pub fn request_hash(input: &SaveQuickAccessInput) -> String {
    // A tuple serializes as a JSON array without reordering the nested struct
    // fields; serde_json::json! would move them into its sorted Map.
    let payload = (
        &input.user_id,
        &input.surface,
        &input.context_key,
        HashPayload {
            pinned_ids: input.pinned_ids.as_ref(),
            expected_version: input.expected_version,
        },
    );
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&payload).expect("serializable shortcut hash input"))
    )
}

pub fn result_json(value: &QuickAccessPreference) -> String {
    serde_json::to_string(&PreferenceWire {
        pinned_ids: value.pinned_ids.clone(),
        version: value.version,
    })
    .expect("serializable shortcut result")
}

fn parse_result(value: &str) -> Result<QuickAccessPreference, BusinessError> {
    let parsed: PreferenceWire = serde_json::from_str(value).map_err(|_| {
        BusinessError::invalid("INVALID_SHORTCUT_RESULT", "Review the saved shortcuts.")
    })?;
    Ok(QuickAccessPreference {
        pinned_ids: parsed.pinned_ids,
        version: parsed.version,
    })
}

pub async fn read_quick_access_preferences<R: QuickAccessRepository>(
    repository: &mut R,
    user_id: &str,
    surface: &str,
    context_key: &str,
) -> Result<QuickAccessPreference, R::Error> {
    if user_id.is_empty() || surface.is_empty() || context_key.is_empty() {
        return Err(
            BusinessError::invalid("INVALID_SCOPE", "The shortcut scope is invalid.").into(),
        );
    }
    Ok(repository
        .read(user_id, surface, context_key)
        .await?
        .unwrap_or(QuickAccessPreference {
            pinned_ids: None,
            version: 0,
        }))
}

pub async fn save_quick_access_preferences<R: QuickAccessRepository>(
    repository: &mut R,
    input: &SaveQuickAccessInput,
    now: i64,
) -> Result<QuickAccessPreference, R::Error> {
    let validated = validate(input)?;
    let hash = request_hash(input);
    if let Some(prior) = repository.receipt(&input.request_key).await? {
        if prior.user_id != input.user_id
            || prior.surface != input.surface
            || prior.context_key != input.context_key
            || prior.request_hash != hash
        {
            return Err(idempotency_conflict().into());
        }
        return parse_result(&prior.result_json).map_err(Into::into);
    }
    let current = repository
        .read(&input.user_id, &input.surface, &input.context_key)
        .await?
        .unwrap_or(QuickAccessPreference {
            pinned_ids: None,
            version: 0,
        });
    if current.version != validated.expected_version {
        return Err(version_conflict().into());
    }
    let current_pins = current.pinned_ids.as_deref().unwrap_or(&[]);
    let available: HashSet<&str> = input.available_ids.iter().map(String::as_str).collect();
    if validated
        .pinned_ids
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|id| !current_pins.contains(id))
        .any(|id| !available.contains(id.as_str()))
    {
        return Err(shortcut_unavailable().into());
    }
    let result = QuickAccessPreference {
        pinned_ids: validated.pinned_ids,
        version: current.version + 1,
    };
    repository
        .save_preference(
            &input.user_id,
            &input.surface,
            &input.context_key,
            &result,
            now,
        )
        .await?;
    repository
        .save_receipt(
            &input.request_key,
            &input.user_id,
            &input.surface,
            &input.context_key,
            &hash,
            &result,
            now,
        )
        .await?;
    Ok(result)
}
