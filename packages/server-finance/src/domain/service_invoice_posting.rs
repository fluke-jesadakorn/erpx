//! Decisions for a retained Field-origin receivable. The outer application must
//! supply current IAM and owner source witnesses; these facts grant no rights.
use super::{journal::JournalLineInput, money::units_to_decimal};
use erpx_server_shared::public::{BusinessError, FailureKind};
use num_bigint::BigInt;

pub struct ServiceInvoicePostFacts<'a> {
    pub version: i64,
    pub subtotal: i64,
    pub tax: i64,
    pub total: i64,
    pub debit_account: &'a str,
    pub credit_account: &'a str,
    pub tax_account: Option<&'a str>,
}
/// Derived from the current sealed selected actor, including same-Person aliases.
/// Apply before replay; independent creator/version/source decisions apply only
/// to fresh effects after immutable receipt lookup, matching the retained API.
pub fn require_service_invoice_poster(
    read: bool,
    write: bool,
    approve: bool,
) -> Result<(), BusinessError> {
    if !write {
        return Err(error(
            FailureKind::Forbidden,
            "PERMISSION_REQUIRED",
            "The current assignment requires finance.write.",
        ));
    }
    if !read {
        return Err(error(
            FailureKind::Forbidden,
            "FINANCE_FORBIDDEN",
            "A finance assignment is required.",
        ));
    }
    if !approve {
        return Err(error(
            FailureKind::Forbidden,
            "FINANCE_APPROVAL_REQUIRED",
            "An independent finance approver is required.",
        ));
    }
    Ok(())
}
/// Fresh source ordering precedes chart lookup and optional review evidence.
pub fn validate_service_invoice_post_source(
    f: &ServiceInvoicePostFacts<'_>,
    expected_version: Option<i64>,
    same_person_as_creator: bool,
) -> Result<(), BusinessError> {
    if same_person_as_creator {
        return Err(error(
            FailureKind::Forbidden,
            "INDEPENDENT_APPROVER_REQUIRED",
            "A different finance approver must post a standalone invoice.",
        ));
    }
    if expected_version.is_some_and(|v| v != f.version) {
        return Err(error(
            FailureKind::Conflict,
            "VERSION_CONFLICT",
            "The invoice changed. Refresh its Debit / Credit before posting.",
        ));
    }
    if f.tax != 0 && f.tax_account.is_none() {
        return Err(error(
            FailureKind::InvalidInput,
            "TAX_ACCOUNT_REQUIRED",
            "This invoice needs a separate tax posting account.",
        ));
    }
    Ok(())
}
pub fn service_invoice_post_lines(
    f: &ServiceInvoicePostFacts<'_>,
    expected_version: Option<i64>,
    same_person_as_creator: bool,
    has_review_attachments: bool,
) -> Result<Vec<JournalLineInput>, BusinessError> {
    validate_service_invoice_post_source(f, expected_version, same_person_as_creator)?;
    if has_review_attachments {
        return Err(error(
            FailureKind::InvalidInput,
            "SUPPLIER_BILL_REQUIRED",
            "Supplier bill review evidence belongs to a payable invoice.",
        ));
    }
    // The retained invoice API refuses an all-zero journal with this wire code.
    // A valid covered zero draft belongs to explicit No-Charge confirmation.
    if f.version >= 1 && f.subtotal == 0 && f.tax == 0 && f.total == 0 {
        return Err(error(
            FailureKind::InvalidInput,
            "INVALID_JOURNAL_LINE",
            "Each line requires one positive debit or credit.",
        ));
    }
    // No floating-point Money or unbalanced posting can leave the owner. Zero
    // charge follows its dedicated existing path, never a synthetic journal.
    if f.version < 1
        || f.subtotal < 0
        || f.tax < 0
        || f.total <= 0
        || f.subtotal.checked_add(f.tax) != Some(f.total)
    {
        return Err(error(
            FailureKind::InvalidInput,
            "INVALID_INVOICE_AMOUNT",
            "Use balanced positive invoice amounts.",
        ));
    }
    if f.debit_account.is_empty() || f.credit_account.is_empty() {
        return Err(error(
            FailureKind::InvalidInput,
            "ACCOUNT_INVALID",
            "Invoice account is unavailable.",
        ));
    }
    let line =
        |account: &str, debit: i64, credit: i64| -> Result<JournalLineInput, BusinessError> {
            let decimal = |v| {
                units_to_decimal(&BigInt::from(v), 2).map_err(|_| {
                    error(
                        FailureKind::InvalidInput,
                        "INVALID_INVOICE_AMOUNT",
                        "Use supported invoice precision.",
                    )
                })
            };
            Ok(JournalLineInput {
                account_id: account.into(),
                description: None,
                debit: decimal(debit)?,
                credit: decimal(credit)?,
                dimensions: None,
            })
        };
    service_invoice_preview_lines(f)?.into_iter().map(|(id,debit,credit)|line(id,debit,credit)).collect()
}
/// Read-only GL amounts share the posting owner's integer rule. A preview
/// neither decides actor admission nor commits a journal.
pub fn service_invoice_preview_lines<'a>(f:&ServiceInvoicePostFacts<'a>)->Result<Vec<(&'a str,i64,i64)>,BusinessError>{
    if f.subtotal<0||f.tax<0||f.total<0||f.subtotal.checked_add(f.tax)!=Some(f.total) {
        return Err(error(FailureKind::InvalidInput,"INVALID_INVOICE_AMOUNT","Use balanced invoice amounts."));
    }
    if f.debit_account.is_empty()||f.credit_account.is_empty()||f.tax!=0&&f.tax_account.is_none_or(str::is_empty) {
        return Err(error(FailureKind::InvalidInput,"ACCOUNT_INVALID","Invoice account is unavailable."));
    }
    let mut lines=vec![(f.debit_account,f.total,0),(f.credit_account,0,f.subtotal)];
    if f.tax!=0 {lines.push((f.tax_account.unwrap(),0,f.tax));}
    Ok(lines)
}
fn error(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}

/// Retained invoice-post request constraints, independent of transport/framework.
pub fn validate_service_invoice_post_request(
    period: &str,
    date: &str,
    version: Option<i64>,
    fx: Option<&str>,
    attachments: &[String],
) -> Result<(), BusinessError> {
    let decimal = |s: &str| {
        let s = s.strip_prefix('-').unwrap_or(s);
        match s.split_once('.') {
            Some((whole, fraction)) => {
                !whole.is_empty()
                    && whole.bytes().all(|b| b.is_ascii_digit())
                    && (1..=6).contains(&fraction.len())
                    && fraction.bytes().all(|b| b.is_ascii_digit())
            }
            None => !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()),
        }
    };
    if period.is_empty()
        || date.len() != 10
        || !time::format_description::parse_borrowed::<2>("[year]-[month]-[day]")
            .is_ok_and(|format| time::Date::parse(date, &format).is_ok())
        || version.is_some_and(|v| !(1..=9_007_199_254_740_991).contains(&v))
        || fx.is_some_and(|s| !decimal(s))
        || attachments.len() > 20
        || attachments.iter().any(String::is_empty)
    {
        return Err(BusinessError::invalid(
            "VALIDATION_FAILED",
            "Posting period, date and a valid exchange rate are required.",
        ));
    }
    Ok(())
}
