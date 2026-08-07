//! Production typed CQRS transports for the Wave 4/5 control contexts.
//!
//! Generated messages are deliberately confined below this module. Feature
//! gateways receive only their owning context's command/query and result
//! types.

// The integrator registers these leaf adapters before the Console composition
// root starts constructing them during the coordinated v1 cutover.
#![allow(dead_code, unused_imports)]

mod brand;
mod commercial;
mod common;
mod customer_authority;
mod identity_access;
mod organization;
mod platform_administration;

pub(crate) use brand::BrandGrpcTransport;
pub(crate) use commercial::CommercialGrpcTransport;
pub(crate) use customer_authority::CustomerAuthorityGrpcTransport;
pub(crate) use identity_access::IdentityAccessGrpcTransport;
pub(crate) use organization::OrganizationGrpcTransport;
pub(crate) use platform_administration::PlatformAdministrationGrpcTransport;
