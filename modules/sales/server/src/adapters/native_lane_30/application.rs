//! Query port and handler for the native Sales customer-note read. No SQL is
//! authored or executed here; the owner postgres adapter owns the statements.

use super::domain::{NoteQuery, NotesResponse};
use erpx_platform_shared_server::public::BusinessError;
use std::{future::Future, pin::Pin};

pub type CustomerNoteReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The port carries only
/// bounded routing/scope facts; no SQL or projection crosses it.
pub trait CustomerNoteRead: Send + Sync {
    fn read<'a>(&'a self, query: &'a NoteQuery) -> CustomerNoteReadFuture<'a, NotesResponse>;
}

/// Validate first, then delegate to the injected owner reader.
pub async fn read_notes<R: CustomerNoteRead + ?Sized>(
    reader: &R,
    query: &NoteQuery,
) -> Result<NotesResponse, BusinessError> {
    query.validate()?;
    reader.read(query).await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CountingReader(std::sync::Mutex<usize>);

    impl CustomerNoteRead for CountingReader {
        fn read<'a>(&'a self, _query: &'a NoteQuery) -> CustomerNoteReadFuture<'a, NotesResponse> {
            *self.0.lock().unwrap() += 1;
            Box::pin(async { Ok(NotesResponse::default()) })
        }
    }

    #[tokio::test]
    async fn handler_validates_before_reader() {
        let reader = CountingReader(std::sync::Mutex::new(0));
        let mut query = NoteQuery {
            company: "company-1".into(),
            branch: None,
            department: None,
            source: "order".into(),
            id: "order-1".into(),
        };
        assert!(read_notes(&reader, &query).await.is_ok());
        assert_eq!(*reader.0.lock().unwrap(), 1);
        query.source = "write".into();
        assert!(read_notes(&reader, &query).await.is_err());
        assert_eq!(
            *reader.0.lock().unwrap(),
            1,
            "invalid input must fail before the reader"
        );
    }
}
