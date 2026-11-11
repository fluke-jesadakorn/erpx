//! Files-owned Messaging attachment port facade.
//!
//! Composition supplies the current session/membership resolver and the private
//! byte store; Files owns the quarantine/scan gate, exact set/link binding and
//! the bounded audit read.
pub use crate::adapters::messaging_files::{
    MAX_MESSAGING_FILE_BYTES, MessagingFileVariant, MessagingFilesActor, MessagingFilesAudience,
    MessagingFilesFuture, PgMessagingFiles, valid_messaging_attachment_source,
};
