//! Native staff portal-support commands.
pub use crate::adapters::portal_support_commands::{PgPortalSupportCommands,SourceRow,ResponseRow};
pub use crate::adapters::portal_support_commands_http::{CustomerAckBackend,CustomerAckRequest,CustomerTarget,StaffBackend,StaffCommand,StaffRequest,SupportFuture,SupportReply,SupportTarget,portal_support_customer_router,portal_support_staff_router};
pub use crate::domain::portal_support_commands::*;
