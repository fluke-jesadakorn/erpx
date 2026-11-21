//! Shared POST branch selection and wire failure provenance. Data only.
use erpx_server_field_service::public::parts::{
    FieldServiceRawOrderTarget, FieldServiceWireFailure, NormalizedFieldServiceParts,
    normalize_field_service_parts,
};
use erpx_server_workflow::public::commands::CommandJson;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OtherPostBranch {
    Request,
    AgreementChange,
    AgreementCreate,
    OrderChange,
    OrderCreate,
}
pub enum SharedPostWire {
    Parts {
        target: FieldServiceRawOrderTarget,
        normalized: NormalizedFieldServiceParts,
    },
    Other {
        branch: OtherPostBranch,
        id_utf16: Option<Vec<u16>>,
    },
}
pub enum SharedPostWireFailure {
    InvalidJson,
    Schema(FieldServiceWireFailure),
}
/// Caller invokes this only after selected Identity. The original immutable
/// bytes remain available for the selected branch and serializable retries.
pub fn classify_shared_post(bytes: &[u8]) -> Result<SharedPostWire, SharedPostWireFailure> {
    let text = String::from_utf8_lossy(bytes);
    let document = CommandJson::parse(text.strip_prefix('\u{feff}').unwrap_or(&text))
        .map_err(|_| SharedPostWireFailure::InvalidJson)?;
    if document.as_str() == "null" {
        return Err(SharedPostWireFailure::InvalidJson);
    }
    let entity = document
        .property("entity")
        .map_err(|_| SharedPostWireFailure::InvalidJson)?
        .and_then(|x| x.string_utf8_storage().ok().flatten());
    let id = document
        .property("id")
        .map_err(|_| SharedPostWireFailure::InvalidJson)?
        .and_then(|x| x.string_utf16_units().ok().flatten());
    if entity.as_deref() == Some("request") {
        return Ok(SharedPostWire::Other {
            branch: OtherPostBranch::Request,
            id_utf16: id,
        });
    }
    if entity.as_deref() == Some("agreement") {
        return Ok(SharedPostWire::Other {
            branch: if id.is_some() {
                OtherPostBranch::AgreementChange
            } else {
                OtherPostBranch::AgreementCreate
            },
            id_utf16: id,
        });
    }
    let Some(id) = id else {
        return Ok(SharedPostWire::Other {
            branch: OtherPostBranch::OrderCreate,
            id_utf16: None,
        });
    };
    let action = document
        .property("action")
        .map_err(|_| SharedPostWireFailure::InvalidJson)?
        .and_then(|x| x.string_utf8_storage().ok().flatten());
    if matches!(action.as_deref(), Some("reserve-part" | "issue-part")) {
        let normalized =
            normalize_field_service_parts(bytes).map_err(SharedPostWireFailure::Schema)?;
        Ok(SharedPostWire::Parts {
            target: FieldServiceRawOrderTarget::from_utf16(id),
            normalized,
        })
    } else {
        Ok(SharedPostWire::Other {
            branch: OtherPostBranch::OrderChange,
            id_utf16: Some(id),
        })
    }
}
