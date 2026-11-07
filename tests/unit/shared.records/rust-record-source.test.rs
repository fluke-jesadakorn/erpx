//! Native Records source-guard port and validation helpers.
//!
//! These prove the owner-local Records source contract used by cross-owner
//! consumers: reference validation, matching source snapshots and the
//! prepare/revalidate port shape. They do not claim a database adapter or
//! owner completion.
use erpx_server_records::public::source_record::{
    PreparedSource, SourceKind, SourceRecord, SourceRecordPort, SourceReference, prepare_source,
    require_matching_source, validate_reference,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::future::Future;
use std::pin::Pin;

fn source() -> SourceRecord {
    SourceRecord {
        kind: SourceKind::Asset,
        id: "source-1".into(),
        company_id: "company".into(),
        branch_id: Some("branch-1".into()),
        department_id: None,
        version: 2,
        title: "Source title".into(),
        status: "open".into(),
    }
}

fn reference(expected_version: Option<i64>) -> SourceReference {
    SourceReference {
        kind: SourceKind::Asset,
        id: "source-1".into(),
        expected_version,
    }
}

#[test]
fn validate_reference_accepts_a_positive_expected_version() {
    assert!(validate_reference(&reference(Some(2))).is_ok());
    assert!(validate_reference(&reference(None)).is_ok());
}

#[test]
fn validate_reference_rejects_empty_oversized_or_nonpositive_versions() {
    let mut bad = reference(None);
    bad.id = String::new();
    assert_eq!(
        validate_reference(&bad).unwrap_err(),
        BusinessError::invalid("SOURCE_RECORD_INVALID", "Provide a valid source record.")
    );

    let mut bad = reference(None);
    bad.id = "x".repeat(181);
    assert!(validate_reference(&bad).is_err());

    let mut bad = reference(Some(0));
    assert!(validate_reference(&bad).is_err());

    let mut bad = reference(Some(-1));
    assert!(validate_reference(&bad).is_err());

    let mut bad = reference(None);
    bad.id = "bad\0id".into();
    assert!(validate_reference(&bad).is_err());
}

#[test]
fn require_matching_source_checks_kind_id_company_and_version() {
    let source = source();
    assert!(require_matching_source(&reference(Some(2)), "company", &source).is_ok());
    assert!(require_matching_source(&reference(None), "company", &source).is_ok());

    let mut wrong_kind = source.clone();
    wrong_kind.kind = SourceKind::PosSale;
    assert_eq!(
        require_matching_source(&reference(Some(2)), "company", &wrong_kind)
            .unwrap_err()
            .code,
        "SOURCE_RECORD_NOT_FOUND"
    );

    let mut wrong_company = source.clone();
    wrong_company.company_id = "other".into();
    assert_eq!(
        require_matching_source(&reference(Some(2)), "company", &wrong_company)
            .unwrap_err()
            .code,
        "SOURCE_RECORD_NOT_FOUND"
    );

    assert_eq!(
        require_matching_source(&reference(Some(3)), "company", &source)
            .unwrap_err()
            .code,
        "SOURCE_RECORD_CHANGED"
    );

    let mut wrong_id = source.clone();
    wrong_id.id = "other".into();
    assert_eq!(
        require_matching_source(&reference(Some(2)), "company", &wrong_id)
            .unwrap_err()
            .code,
        "SOURCE_RECORD_NOT_FOUND"
    );
}

struct FakePort {
    record: SourceRecord,
    witness: i64,
    current: bool,
    prepare_calls: usize,
    require_calls: usize,
}

impl SourceRecordPort for FakePort {
    type Error = BusinessError;
    type Witness = i64;

    fn prepare(
        &mut self,
        source: &SourceReference,
    ) -> Pin<Box<dyn Future<Output = Result<(SourceRecord, Self::Witness), Self::Error>> + Send + '_>>
    {
        self.prepare_calls += 1;
        let record = self.record.clone();
        let witness = self.witness;
        let matches = record.kind == source.kind && record.id == source.id;
        Box::pin(async move {
            if !matches {
                return Err(BusinessError::new(
                    FailureKind::NotFound,
                    "SOURCE_RECORD_NOT_FOUND",
                    "Source record is unavailable in this work scope.",
                ));
            }
            Ok((record, witness))
        })
    }

    fn require_current(
        &mut self,
        witness: &Self::Witness,
    ) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>> {
        self.require_calls += 1;
        let current = self.current && *witness == self.witness;
        Box::pin(async move {
            if current {
                Ok(())
            } else {
                Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SOURCE_RECORD_CHANGED",
                    "The source version changed. Reload its owning workspace before handing it off.",
                ))
            }
        })
    }
}

#[tokio::test]
async fn prepare_source_validates_and_revalidates_through_the_port() {
    let source = source();
    let mut port = FakePort {
        record: source.clone(),
        witness: 42,
        current: true,
        prepare_calls: 0,
        require_calls: 0,
    };

    let prepared = prepare_source(&mut port, "company", &reference(Some(2)))
        .await
        .unwrap();
    assert_eq!(prepared.record, source);
    assert_eq!(port.prepare_calls, 1);

    let (record, witness) = prepared.into_parts();
    assert_eq!(record, source);
    assert_eq!(witness, 42);

    let mut port = FakePort {
        record: source.clone(),
        witness: 42,
        current: true,
        prepare_calls: 0,
        require_calls: 0,
    };
    let prepared = prepare_source(&mut port, "company", &reference(Some(2)))
        .await
        .unwrap();
    prepared.require_current(&mut port).await.unwrap();
    assert_eq!(port.require_calls, 1);

    let mut port = FakePort {
        record: source.clone(),
        witness: 42,
        current: false,
        prepare_calls: 0,
        require_calls: 0,
    };
    let prepared = prepare_source(&mut port, "company", &reference(Some(2)))
        .await
        .unwrap();
    assert!(prepared.require_current(&mut port).await.is_err());
}

#[tokio::test]
async fn prepare_source_rejects_invalid_reference_before_calling_port() {
    let source = source();
    let mut port = FakePort {
        record: source,
        witness: 42,
        current: true,
        prepare_calls: 0,
        require_calls: 0,
    };
    let bad = SourceReference {
        kind: SourceKind::Asset,
        id: String::new(),
        expected_version: None,
    };
    assert!(prepare_source(&mut port, "company", &bad).await.is_err());
    assert_eq!(port.prepare_calls, 0);
}

#[tokio::test]
async fn prepare_source_rejects_mismatched_source_returned_by_port() {
    let source = source();
    let mut port = FakePort {
        record: source,
        witness: 42,
        current: true,
        prepare_calls: 0,
        require_calls: 0,
    };
    let bad_reference = SourceReference {
        kind: SourceKind::Asset,
        id: "different".into(),
        expected_version: None,
    };
    assert!(prepare_source(&mut port, "company", &bad_reference).await.is_err());
    assert_eq!(port.prepare_calls, 1);
}

#[tokio::test]
async fn prepare_source_rejects_version_mismatch() {
    let source = source();
    let mut port = FakePort {
        record: source,
        witness: 42,
        current: true,
        prepare_calls: 0,
        require_calls: 0,
    };
    let bad = SourceReference {
        kind: SourceKind::Asset,
        id: "source-1".into(),
        expected_version: Some(99),
    };
    let error = match prepare_source(&mut port, "company", &bad).await {
        Err(error) => error,
        Ok(_) => panic!("expected source version mismatch"),
    };
    assert_eq!(error.code, "SOURCE_RECORD_CHANGED");
}
