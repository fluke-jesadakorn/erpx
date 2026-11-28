//! Pure URL serialization helpers; these values grant no authentication or authority.
pub use crate::adapters::oidc_url_values::{
    encode_uri_component, encode_uri_component_utf16, set_authorization_parameters,
};
