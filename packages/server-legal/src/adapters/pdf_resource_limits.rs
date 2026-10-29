//! Called by the dedicated executable before reading any untrusted input. Never
//! call this in the API, shared jobs runtime or test runner process.
use crate::domain::pdf_verification::{PdfVerificationFailure, VERIFICATION_DEADLINE_SECONDS};

pub const PDF_CHILD_DATA_LIMIT: u64 = 512 * 1024 * 1024;

#[cfg(unix)]
pub fn enforce_pdf_child_resource_limits() -> Result<(), PdfVerificationFailure> {
    use nix::sys::resource::{Resource, setrlimit};
    for (resource, ceiling) in [
        (Resource::RLIMIT_CPU, VERIFICATION_DEADLINE_SECONDS),
        (Resource::RLIMIT_STACK, 16 * 1024 * 1024),
        (Resource::RLIMIT_NOFILE, 64),
        (Resource::RLIMIT_FSIZE, 0),
        (Resource::RLIMIT_CORE, 0),
    ] {
        setrlimit(resource, ceiling, ceiling).map_err(|_| {
            eprintln!("VERIFIER_RESOURCE_LIMIT_UNAVAILABLE:{resource:?}");
            PdfVerificationFailure::WorkerUnavailable
        })?;
    }
    // Actual disposable-child checks on this Mac reject DATA=512MiB before
    // parsing. It maps large shared regions; these are not a resident-memory
    // ceiling. The supervisor measures owned-child RSS and kills on overflow.
    #[cfg(target_os = "linux")]
    {
        setrlimit(
            Resource::RLIMIT_DATA,
            PDF_CHILD_DATA_LIMIT,
            PDF_CHILD_DATA_LIMIT,
        )
        .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
        setrlimit(Resource::RLIMIT_AS, 1024 * 1024 * 1024, 1024 * 1024 * 1024)
            .map_err(|_| PdfVerificationFailure::WorkerUnavailable)?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn enforce_pdf_child_resource_limits() -> Result<(), PdfVerificationFailure> {
    Err(PdfVerificationFailure::WorkerUnavailable)
}
