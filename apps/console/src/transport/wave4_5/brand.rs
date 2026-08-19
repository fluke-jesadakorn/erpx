use async_trait::async_trait;
use erpx_brand::{
    BrandAssetKind, BrandAssetManifest, BrandAssetReference, BrandCategory, BrandDefinition,
    BrandDensity, BrandElevation, BrandFontPack, BrandHistory, BrandHistoryEntry, BrandLifecycle,
    BrandMotion, BrandMutationResult, BrandNavigation, BrandPalette, BrandProfileId,
    BrandProfileView, BrandRadius, BrandRevision, BrandScope, BrandTokenSet, BusinessUnitKey,
    CreateBrandDraft, CreateBrandDraftResult, GetBrandDraft, GetBrandDraftResult, GetResolvedBrand,
    GetResolvedBrandResult, HexColor, ListBrandHistory, PublishBrandDraft, ResolvedBrandView,
    RollbackBrand, SaveBrandDraft, SetBrandCategoryLocks,
};
use erpx_cqrs::{CommandEnvelope, CommandReceipt, QueryEnvelope, QueryResult};
use erpx_proto::erpx::brand as platform;
use erpx_ui_foundation::UiError;

use crate::{features::brand::gateway::BrandProtoTransport, transport::typed_cqrs::GrpcChannel};

use super::common::{
    RpcAuthority, command_receipt, invalid_response, query_result, rpc, time, uuid,
};

#[derive(Clone)]
pub(crate) struct BrandGrpcTransport {
    command: platform::brand_command_service_client::BrandCommandServiceClient<GrpcChannel>,
    query: platform::brand_query_service_client::BrandQueryServiceClient<GrpcChannel>,
    authority: RpcAuthority,
}

impl BrandGrpcTransport {
    pub(crate) fn try_new(
        channel: GrpcChannel,
        access_token: impl AsRef<str>,
        holding_id: impl AsRef<str>,
        data_profile_key: impl AsRef<str>,
    ) -> Result<Self, UiError> {
        Ok(Self {
            command: platform::brand_command_service_client::BrandCommandServiceClient::new(
                channel.clone(),
            ),
            query: platform::brand_query_service_client::BrandQueryServiceClient::new(channel),
            authority: RpcAuthority::holding(access_token, holding_id, data_profile_key)?,
        })
    }
}

fn profile_receipt<R>(
    receipt: Option<erpx_proto::common::CommandReceipt>,
    profile: Option<platform::BrandProfileRecord>,
    result: impl FnOnce(BrandProfileView) -> R,
) -> Result<CommandReceipt<R>, UiError> {
    command_receipt(
        receipt,
        result(decode_profile(profile.ok_or_else(invalid_response)?)?),
    )
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl BrandProtoTransport for BrandGrpcTransport {
    async fn create_draft(
        &self,
        envelope: CommandEnvelope<CreateBrandDraft>,
    ) -> Result<CommandReceipt<CreateBrandDraftResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::CreateBrandDraftRequest {
                metadata: Some(metadata),
                profile_id: command.profile_id.as_uuid().to_string(),
                scope: Some(encode_scope(&command.scope)),
                definition: Some(encode_definition(&command.definition)),
                reason: command.reason,
            }
        })?;
        let response = self
            .command
            .clone()
            .create_draft(request)
            .await
            .map_err(|error| rpc(error, "create brand draft"))?
            .into_inner();
        profile_receipt(response.receipt, response.profile, |profile| {
            CreateBrandDraftResult { profile }
        })
    }

    async fn save_draft(
        &self,
        envelope: CommandEnvelope<SaveBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::SaveBrandDraftRequest {
                metadata: Some(metadata),
                profile_id: command.profile_id.as_uuid().to_string(),
                definition: Some(encode_definition(&command.definition)),
                reason: command.reason,
            }
        })?;
        let response = self
            .command
            .clone()
            .save_draft(request)
            .await
            .map_err(|error| rpc(error, "save brand draft"))?
            .into_inner();
        profile_receipt(response.receipt, response.profile, |profile| {
            BrandMutationResult { profile }
        })
    }

    async fn publish_draft(
        &self,
        envelope: CommandEnvelope<PublishBrandDraft>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::PublishBrandDraftRequest {
                metadata: Some(metadata),
                profile_id: command.profile_id.as_uuid().to_string(),
                reason: command.reason,
            }
        })?;
        let response = self
            .command
            .clone()
            .publish_draft(request)
            .await
            .map_err(|error| rpc(error, "publish brand draft"))?
            .into_inner();
        profile_receipt(response.receipt, response.profile, |profile| {
            BrandMutationResult { profile }
        })
    }

    async fn rollback(
        &self,
        envelope: CommandEnvelope<RollbackBrand>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::RollbackBrandRequest {
                metadata: Some(metadata),
                profile_id: command.profile_id.as_uuid().to_string(),
                source_revision: command.source_revision,
                reason: command.reason,
            }
        })?;
        let response = self
            .command
            .clone()
            .rollback(request)
            .await
            .map_err(|error| rpc(error, "rollback brand"))?
            .into_inner();
        profile_receipt(response.receipt, response.profile, |profile| {
            BrandMutationResult { profile }
        })
    }

    async fn set_category_locks(
        &self,
        envelope: CommandEnvelope<SetBrandCategoryLocks>,
    ) -> Result<CommandReceipt<BrandMutationResult>, UiError> {
        let request = self.authority.command(envelope, |command, metadata| {
            platform::SetBrandCategoryLocksRequest {
                metadata: Some(metadata),
                profile_id: command.profile_id.as_uuid().to_string(),
                locked_categories: command
                    .locked_categories
                    .into_iter()
                    .map(encode_category)
                    .collect(),
                reason: command.reason,
            }
        })?;
        let response = self
            .command
            .clone()
            .set_category_locks(request)
            .await
            .map_err(|error| rpc(error, "set brand category locks"))?
            .into_inner();
        profile_receipt(response.receipt, response.profile, |profile| {
            BrandMutationResult { profile }
        })
    }

    async fn get_draft(
        &self,
        envelope: QueryEnvelope<GetBrandDraft>,
    ) -> Result<QueryResult<GetBrandDraftResult>, UiError> {
        let request =
            self.authority
                .query(envelope, |query, metadata| platform::GetBrandDraftRequest {
                    metadata: Some(metadata),
                    scope: Some(encode_scope(&query.scope)),
                })?;
        let response = self
            .query
            .clone()
            .get_draft(request)
            .await
            .map_err(|error| rpc(error, "get brand draft"))?
            .into_inner();
        let profile = response.profile.map(decode_profile).transpose()?;
        query_result(response.metadata, GetBrandDraftResult { profile })
    }

    async fn get_resolved(
        &self,
        envelope: QueryEnvelope<GetResolvedBrand>,
    ) -> Result<QueryResult<GetResolvedBrandResult>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::GetResolvedBrandRequest {
                metadata: Some(metadata),
                scope: Some(encode_scope(&query.scope)),
            }
        })?;
        let response = self
            .query
            .clone()
            .get_resolved(request)
            .await
            .map_err(|error| rpc(error, "get resolved brand"))?
            .into_inner();
        let brand = response.brand.map(decode_resolved).transpose()?;
        query_result(response.metadata, GetResolvedBrandResult { brand })
    }

    async fn list_history(
        &self,
        envelope: QueryEnvelope<ListBrandHistory>,
    ) -> Result<QueryResult<BrandHistory>, UiError> {
        let request = self.authority.query(envelope, |query, metadata| {
            platform::ListBrandHistoryRequest {
                metadata: Some(metadata),
                scope: Some(encode_scope(&query.scope)),
                page_size: u32::from(query.page_size),
                cursor: query.cursor,
            }
        })?;
        let response = self
            .query
            .clone()
            .list_history(request)
            .await
            .map_err(|error| rpc(error, "list brand history"))?
            .into_inner();
        let entries = response
            .entries
            .into_iter()
            .map(decode_history)
            .collect::<Result<_, _>>()?;
        query_result(
            response.metadata,
            BrandHistory {
                entries,
                next_cursor: response.next_cursor,
            },
        )
    }
}

fn encode_scope(value: &BrandScope) -> platform::BrandTargetScope {
    use platform::brand_target_scope::Target;
    let target = match value {
        BrandScope::Platform => Target::Platform(platform::BrandPlatformTarget {}),
        BrandScope::Holding {
            holding_id,
            holding_id,
        } => Target::Holding(platform::BrandHoldingTarget {
            holding_id: holding_id.clone(),
            holding_id: holding_id.to_string(),
        }),
        BrandScope::BusinessUnit {
            holding_id,
            holding_id,
            business_unit,
        } => Target::BusinessUnit(platform::BrandBusinessUnitTarget {
            holding_id: holding_id.clone(),
            holding_id: holding_id.to_string(),
            business_unit: encode_business_unit(*business_unit),
        }),
    };
    platform::BrandTargetScope {
        target: Some(target),
    }
}

fn decode_scope(value: platform::BrandTargetScope) -> Result<BrandScope, UiError> {
    use platform::brand_target_scope::Target;
    let scope = match value.target.ok_or_else(invalid_response)? {
        Target::Platform(_) => BrandScope::Platform,
        Target::Holding(value) => BrandScope::Holding {
            holding_id: value.holding_id,
            holding_id: uuid(&value.holding_id)?,
        },
        Target::BusinessUnit(value) => BrandScope::BusinessUnit {
            holding_id: value.holding_id,
            holding_id: uuid(&value.holding_id)?,
            business_unit: decode_business_unit(value.business_unit)?,
        },
    };
    scope.validate().map_err(|_| invalid_response())?;
    Ok(scope)
}

fn encode_business_unit(value: BusinessUnitKey) -> i32 {
    match value {
        BusinessUnitKey::Operations => platform::BrandBusinessUnitKind::Operations as i32,
        BusinessUnitKey::Finance => platform::BrandBusinessUnitKind::Finance as i32,
        BusinessUnitKey::Sales => platform::BrandBusinessUnitKind::Sales as i32,
        BusinessUnitKey::Procurement => platform::BrandBusinessUnitKind::Procurement as i32,
        BusinessUnitKey::Inventory => platform::BrandBusinessUnitKind::Inventory as i32,
        BusinessUnitKey::Assets => platform::BrandBusinessUnitKind::Assets as i32,
        BusinessUnitKey::Maintenance => platform::BrandBusinessUnitKind::Maintenance as i32,
        BusinessUnitKey::Quality => platform::BrandBusinessUnitKind::Quality as i32,
        BusinessUnitKey::MarketingCrm => platform::BrandBusinessUnitKind::MarketingCrm as i32,
        BusinessUnitKey::HrPayroll => platform::BrandBusinessUnitKind::HrPayroll as i32,
        BusinessUnitKey::PointOfSale => platform::BrandBusinessUnitKind::PointOfSale as i32,
        BusinessUnitKey::Legal => platform::BrandBusinessUnitKind::Legal as i32,
    }
}

fn decode_business_unit(value: i32) -> Result<BusinessUnitKey, UiError> {
    match platform::BrandBusinessUnitKind::try_from(value).ok() {
        Some(platform::BrandBusinessUnitKind::Operations) => Ok(BusinessUnitKey::Operations),
        Some(platform::BrandBusinessUnitKind::Finance) => Ok(BusinessUnitKey::Finance),
        Some(platform::BrandBusinessUnitKind::Sales) => Ok(BusinessUnitKey::Sales),
        Some(platform::BrandBusinessUnitKind::Procurement) => Ok(BusinessUnitKey::Procurement),
        Some(platform::BrandBusinessUnitKind::Inventory) => Ok(BusinessUnitKey::Inventory),
        Some(platform::BrandBusinessUnitKind::Assets) => Ok(BusinessUnitKey::Assets),
        Some(platform::BrandBusinessUnitKind::Maintenance) => Ok(BusinessUnitKey::Maintenance),
        Some(platform::BrandBusinessUnitKind::Quality) => Ok(BusinessUnitKey::Quality),
        Some(platform::BrandBusinessUnitKind::MarketingCrm) => Ok(BusinessUnitKey::MarketingCrm),
        Some(platform::BrandBusinessUnitKind::HrPayroll) => Ok(BusinessUnitKey::HrPayroll),
        Some(platform::BrandBusinessUnitKind::PointOfSale) => Ok(BusinessUnitKey::PointOfSale),
        Some(platform::BrandBusinessUnitKind::Legal) => Ok(BusinessUnitKey::Legal),
        _ => Err(invalid_response()),
    }
}

fn encode_definition(value: &BrandDefinition) -> platform::BrandDefinition {
    platform::BrandDefinition {
        display_name: value.display_name.clone(),
        short_name: value.short_name.clone(),
        tokens: Some(encode_tokens(&value.tokens)),
        assets: Some(encode_assets(&value.assets)),
        overridden_categories: value
            .overridden_categories
            .iter()
            .copied()
            .map(encode_category)
            .collect(),
        locked_categories: value
            .locked_categories
            .iter()
            .copied()
            .map(encode_category)
            .collect(),
    }
}

fn decode_definition(value: platform::BrandDefinition) -> Result<BrandDefinition, UiError> {
    Ok(BrandDefinition {
        display_name: value.display_name,
        short_name: value.short_name,
        tokens: decode_tokens(value.tokens.ok_or_else(invalid_response)?)?,
        assets: decode_assets(value.assets.ok_or_else(invalid_response)?)?,
        overridden_categories: value
            .overridden_categories
            .into_iter()
            .map(decode_category)
            .collect::<Result<_, _>>()?,
        locked_categories: value
            .locked_categories
            .into_iter()
            .map(decode_category)
            .collect::<Result<_, _>>()?,
    })
}

fn encode_palette(value: &BrandPalette) -> platform::BrandCqrsPalette {
    platform::BrandCqrsPalette {
        primary: value.primary.as_str().into(),
        on_primary: value.on_primary.as_str().into(),
        accent: value.accent.as_str().into(),
        on_accent: value.on_accent.as_str().into(),
        canvas: value.canvas.as_str().into(),
        surface: value.surface.as_str().into(),
        surface_raised: value.surface_raised.as_str().into(),
        border: value.border.as_str().into(),
        text: value.text.as_str().into(),
        text_muted: value.text_muted.as_str().into(),
        link: value.link.as_str().into(),
        focus: value.focus.as_str().into(),
    }
}

fn decode_palette(value: platform::BrandCqrsPalette) -> Result<BrandPalette, UiError> {
    let color = |value| HexColor::try_new(value).map_err(|_| invalid_response());
    Ok(BrandPalette {
        primary: color(value.primary)?,
        on_primary: color(value.on_primary)?,
        accent: color(value.accent)?,
        on_accent: color(value.on_accent)?,
        canvas: color(value.canvas)?,
        surface: color(value.surface)?,
        surface_raised: color(value.surface_raised)?,
        border: color(value.border)?,
        text: color(value.text)?,
        text_muted: color(value.text_muted)?,
        link: color(value.link)?,
        focus: color(value.focus)?,
    })
}

fn encode_tokens(value: &BrandTokenSet) -> platform::BrandCqrsTokenSet {
    platform::BrandCqrsTokenSet {
        light: Some(encode_palette(&value.light)),
        dark: Some(encode_palette(&value.dark)),
        font_pack: encode_font(value.font_pack),
        heading_scale_percent: u32::from(value.heading_scale_percent),
        body_scale_percent: u32::from(value.body_scale_percent),
        line_height_percent: u32::from(value.line_height_percent),
        density: encode_density(value.density),
        radius: encode_radius(value.radius),
        elevation: encode_elevation(value.elevation),
        navigation: encode_navigation(value.navigation),
        navigation_expanded_by_default: value.navigation_expanded_by_default,
        motion: encode_motion(value.motion),
    }
}

fn decode_tokens(value: platform::BrandCqrsTokenSet) -> Result<BrandTokenSet, UiError> {
    Ok(BrandTokenSet {
        light: decode_palette(value.light.ok_or_else(invalid_response)?)?,
        dark: decode_palette(value.dark.ok_or_else(invalid_response)?)?,
        font_pack: decode_font(value.font_pack)?,
        heading_scale_percent: u16::try_from(value.heading_scale_percent)
            .map_err(|_| invalid_response())?,
        body_scale_percent: u16::try_from(value.body_scale_percent)
            .map_err(|_| invalid_response())?,
        line_height_percent: u16::try_from(value.line_height_percent)
            .map_err(|_| invalid_response())?,
        density: decode_density(value.density)?,
        radius: decode_radius(value.radius)?,
        elevation: decode_elevation(value.elevation)?,
        navigation: decode_navigation(value.navigation)?,
        navigation_expanded_by_default: value.navigation_expanded_by_default,
        motion: decode_motion(value.motion)?,
    })
}

fn encode_assets(value: &BrandAssetManifest) -> platform::BrandCqrsAssetManifest {
    platform::BrandCqrsAssetManifest {
        logo_mark: value.logo_mark.as_ref().map(encode_asset),
        wordmark: value.wordmark.as_ref().map(encode_asset),
        favicon: value.favicon.as_ref().map(encode_asset),
        login_artwork: value.login_artwork.as_ref().map(encode_asset),
    }
}

fn decode_assets(value: platform::BrandCqrsAssetManifest) -> Result<BrandAssetManifest, UiError> {
    Ok(BrandAssetManifest {
        logo_mark: value.logo_mark.map(decode_asset).transpose()?,
        wordmark: value.wordmark.map(decode_asset).transpose()?,
        favicon: value.favicon.map(decode_asset).transpose()?,
        login_artwork: value.login_artwork.map(decode_asset).transpose()?,
    })
}

fn encode_asset(value: &BrandAssetReference) -> platform::BrandCqrsAssetReference {
    platform::BrandCqrsAssetReference {
        kind: encode_asset_kind(value.kind),
        object_key: value.object_key.clone(),
        sha256: value.sha256.clone(),
        media_type: value.media_type.clone(),
        byte_length: value.byte_length,
        width: value.width,
        height: value.height,
    }
}

fn decode_asset(value: platform::BrandCqrsAssetReference) -> Result<BrandAssetReference, UiError> {
    Ok(BrandAssetReference {
        kind: decode_asset_kind(value.kind)?,
        object_key: value.object_key,
        sha256: value.sha256,
        media_type: value.media_type,
        byte_length: value.byte_length,
        width: value.width,
        height: value.height,
    })
}

fn decode_profile(value: platform::BrandProfileRecord) -> Result<BrandProfileView, UiError> {
    let scope = decode_scope(value.scope.ok_or_else(invalid_response)?)?;
    let draft = value
        .draft
        .map(|value| decode_revision(&scope, value))
        .transpose()?;
    let published = value
        .published
        .map(|value| decode_revision(&scope, value))
        .transpose()?;
    Ok(BrandProfileView {
        profile_id: BrandProfileId::try_from_uuid(uuid(&value.profile_id)?)
            .map_err(|_| invalid_response())?,
        aggregate_version: value.aggregate_version,
        scope,
        draft,
        published,
        created_at: time(&value.created_at)?,
        updated_at: time(&value.updated_at)?,
    })
}

fn decode_revision(
    scope: &BrandScope,
    value: platform::BrandRevisionRecord,
) -> Result<BrandRevision, UiError> {
    let definition = decode_definition(value.definition.ok_or_else(invalid_response)?)?;
    BrandRevision::try_rehydrate(
        scope,
        value.revision,
        decode_lifecycle(value.lifecycle)?,
        definition,
        value.parent_published_revision,
        value.updated_by,
        time(&value.updated_at)?,
    )
    .map_err(|_| invalid_response())
}

fn decode_resolved(value: platform::ResolvedBrandRecord) -> Result<ResolvedBrandView, UiError> {
    Ok(ResolvedBrandView {
        scope: decode_scope(value.scope.ok_or_else(invalid_response)?)?,
        published_revision: value.published_revision,
        definition: decode_definition(value.definition.ok_or_else(invalid_response)?)?,
        inherited_categories: value
            .inherited_categories
            .into_iter()
            .map(decode_category)
            .collect::<Result<_, _>>()?,
        effective_locked_categories: value
            .effective_locked_categories
            .into_iter()
            .map(decode_category)
            .collect::<Result<_, _>>()?,
        digest: value.digest,
    })
}

fn decode_history(value: platform::BrandHistoryRecord) -> Result<BrandHistoryEntry, UiError> {
    Ok(BrandHistoryEntry {
        profile_id: BrandProfileId::try_from_uuid(uuid(&value.profile_id)?)
            .map_err(|_| invalid_response())?,
        revision: value.revision,
        lifecycle: decode_lifecycle(value.lifecycle)?,
        definition: decode_definition(value.definition.ok_or_else(invalid_response)?)?,
        parent_published_revision: value.parent_published_revision,
        updated_by: value.updated_by,
        reason: value.reason,
        created_at: time(&value.created_at)?,
        published_at: value.published_at.map(|v| time(&v)).transpose()?,
        archived_at: value.archived_at.map(|v| time(&v)).transpose()?,
    })
}

macro_rules! enum_map {
    ($encode:ident, $decode:ident, $domain:ty, $wire:ty, { $($d:ident => $w:ident),+ $(,)? }) => {
        fn $encode(value: $domain) -> i32 { match value { $(<$domain>::$d => <$wire>::$w as i32),+ } }
        fn $decode(value: i32) -> Result<$domain, UiError> { match <$wire>::try_from(value).ok() { $(Some(<$wire>::$w) => Ok(<$domain>::$d)),+, _ => Err(invalid_response()) } }
    };
}

enum_map!(encode_category, decode_category, BrandCategory, platform::BrandCategory, { Identity => Identity, Color => Color, Typography => Typography, Shape => Shape, Elevation => Elevation, Density => Density, Navigation => Navigation, Motion => Motion });
enum_map!(encode_font, decode_font, BrandFontPack, platform::BrandFontPack, { ErpxSans => ErpxSans, CorporateSans => CorporateSans, ThaiFormal => ThaiFormal, SystemUi => SystemUi });
enum_map!(encode_density, decode_density, BrandDensity, platform::BrandDensity, { Compact => Compact, Comfortable => Comfortable, Spacious => Spacious });
enum_map!(encode_radius, decode_radius, BrandRadius, platform::BrandRadius, { Square => Square, Soft => Soft, Rounded => Rounded });
enum_map!(encode_elevation, decode_elevation, BrandElevation, platform::BrandElevation, { Flat => Flat, Subtle => Subtle, Elevated => Elevated });
enum_map!(encode_navigation, decode_navigation, BrandNavigation, platform::BrandNavigation, { Solid => Solid, Tinted => Tinted });
enum_map!(encode_motion, decode_motion, BrandMotion, platform::BrandMotion, { None => None, Subtle => Subtle, Standard => Standard });
enum_map!(encode_asset_kind, decode_asset_kind, BrandAssetKind, platform::BrandAssetKindValue, { LogoMark => LogoMark, Wordmark => Wordmark, Favicon => Favicon, LoginArtwork => LoginArtwork });
fn decode_lifecycle(value: i32) -> Result<BrandLifecycle, UiError> {
    match platform::BrandLifecycleState::try_from(value).ok() {
        Some(platform::BrandLifecycleState::Draft) => Ok(BrandLifecycle::Draft),
        Some(platform::BrandLifecycleState::Published) => Ok(BrandLifecycle::Published),
        _ => Err(invalid_response()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unspecified_brand_enum_is_rejected() {
        assert!(decode_category(0).is_err());
        assert!(decode_business_unit(0).is_err());
        assert!(decode_lifecycle(0).is_err());
    }
}
