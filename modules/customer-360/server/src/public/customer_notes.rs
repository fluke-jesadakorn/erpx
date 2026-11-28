pub use crate::adapters::notes::{
    CustomerNotesReadActor, lock_customer_note_sources, read_customer_notes,
};
pub use crate::domain::notes::{
    ChangeCustomerNoteInput, CreateCustomerNoteInput, CustomerNoteCommandResult,
    CustomerNoteHistoryRow, CustomerNoteOrder, CustomerNoteRow, CustomerNoteSource,
    CustomerNotesView, NOTE_BODY_MAX, NOTE_BODY_MIN, NOTE_CHANNELS, customer_unavailable,
    note_changed, note_owner_required, note_unavailable,
};
