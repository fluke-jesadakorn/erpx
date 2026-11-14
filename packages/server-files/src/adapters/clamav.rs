use crate::domain::canonical_original::ORIGINAL_LIMIT;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind, TimestampMillis};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    sync::Semaphore,
};

#[derive(Clone)]
pub struct ClamAvConfig {
    pub socket: PathBuf,
    pub database_directory: PathBuf,
    pub maximum_definition_age: Duration,
    pub command_timeout: Duration,
    pub scan_timeout: Duration,
    pub concurrent_scans: usize,
}

use crate::domain::malware_scan::{MalwareScan, MalwareVerdict, ScannerReadiness};

/// The only destination is a trusted configured local daemon socket. Input
/// filenames and document URLs are never sent or interpreted as commands.
#[derive(Clone)]
pub struct ClamAvScanner {
    config: ClamAvConfig,
    admission: Arc<Semaphore>,
}
impl ClamAvScanner {
    pub fn new(config: ClamAvConfig) -> Result<Self, BusinessError> {
        if !config.socket.is_absolute()
            || !config.database_directory.is_absolute()
            || !(1..=16).contains(&config.concurrent_scans)
            || config.maximum_definition_age < Duration::from_secs(60)
            || config.maximum_definition_age.as_millis() > 9_007_199_254_740_991
            || config.command_timeout < Duration::from_millis(25)
            || config.command_timeout > Duration::from_secs(5)
            || config.scan_timeout < Duration::from_millis(25)
            || config.scan_timeout > Duration::from_secs(30)
        {
            return Err(failure("CLAMD_CONFIGURATION"));
        }
        Ok(Self {
            admission: Arc::new(Semaphore::new(config.concurrent_scans)),
            config,
        })
    }

    pub async fn readiness(
        &self,
        clock: &(impl Clock + Sync),
    ) -> Result<ScannerReadiness, BusinessError> {
        tokio::time::timeout(self.config.command_timeout, async {
            let (pong, engine, definition) = tokio::try_join!(
                self.command("PING"),
                self.command("VERSION"),
                self.newest_definition()
            )?;
            if pong != "PONG" || !engine.starts_with("ClamAV ") {
                return Err(failure("CLAMD_UNHEALTHY"));
            }
            let age = clock.now().get().saturating_sub(definition.get());
            if age > self.config.maximum_definition_age.as_millis() as i64 {
                return Err(failure("CLAMAV_DEFINITIONS_STALE"));
            }
            Ok(ScannerReadiness {
                engine,
                definition_updated_at: definition,
            })
        })
        .await
        .map_err(|_| failure("CLAMD_COMMAND_TIMEOUT"))?
    }

    pub async fn scan(
        &self,
        bytes: &[u8],
        clock: &(impl Clock + Sync),
    ) -> Result<MalwareScan, BusinessError> {
        if bytes.is_empty() || bytes.len() > ORIGINAL_LIMIT {
            return Err(failure("CLAMD_STREAM_LIMIT"));
        }
        // Admission, readiness, writes and the response share one total deadline.
        // Dropping this future closes the socket and releases its owned permit.
        tokio::time::timeout(self.config.scan_timeout, async {
            let _permit = self
                .admission
                .acquire()
                .await
                .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
            let readiness = self.readiness(clock).await?;
            let mut socket = UnixStream::connect(&self.config.socket)
                .await
                .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
            socket
                .write_all(b"zINSTREAM\0")
                .await
                .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
            for chunk in bytes.chunks(64 * 1024) {
                socket
                    .write_all(&(chunk.len() as u32).to_be_bytes())
                    .await
                    .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
                socket
                    .write_all(chunk)
                    .await
                    .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
            }
            socket
                .write_all(&[0; 4])
                .await
                .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
            let response = read_response(&mut socket).await?;
            let verdict = if response == "stream: OK" {
                MalwareVerdict::Clean
            } else if let Some(signature) = response
                .strip_prefix("stream: ")
                .and_then(|s| s.strip_suffix(" FOUND"))
            {
                if signature.is_empty() || signature.contains(['\r', '\n']) {
                    return Err(failure("CLAMD_SCAN_RESPONSE"));
                }
                MalwareVerdict::Infected {
                    signature: signature.to_owned(),
                }
            } else {
                return Err(failure("CLAMD_SCAN_RESPONSE"));
            };
            Ok(MalwareScan { verdict, readiness })
        })
        .await
        .map_err(|_| failure("CLAMD_SCAN_TIMEOUT"))?
    }

    async fn command(&self, command: &str) -> Result<String, BusinessError> {
        let mut socket = UnixStream::connect(&self.config.socket)
            .await
            .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
        socket
            .write_all(format!("z{command}\0").as_bytes())
            .await
            .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
        read_response(&mut socket).await
    }

    async fn newest_definition(&self) -> Result<TimestampMillis, BusinessError> {
        let mut latest = 0i64;
        for name in ["daily.cvd", "daily.cld", "main.cvd", "main.cld"] {
            if let Ok(meta) = tokio::fs::metadata(self.config.database_directory.join(name)).await {
                if !meta.is_file() {
                    continue;
                }
                if let Ok(when) = meta.modified().and_then(|time| {
                    time.duration_since(UNIX_EPOCH)
                        .map_err(std::io::Error::other)
                }) {
                    let millis = i64::try_from(when.as_millis())
                        .map_err(|_| failure("CLAMAV_DEFINITIONS_MISSING"))?;
                    latest = latest.max(millis);
                }
            }
        }
        if latest == 0 {
            return Err(failure("CLAMAV_DEFINITIONS_MISSING"));
        }
        Ok(TimestampMillis::new(latest))
    }
}

async fn read_response(socket: &mut UnixStream) -> Result<String, BusinessError> {
    let mut response = Vec::with_capacity(256);
    let mut chunk = [0u8; 256];
    loop {
        let count = socket
            .read(&mut chunk)
            .await
            .map_err(|_| failure("CLAMD_UNAVAILABLE"))?;
        if count == 0 {
            return Err(failure("CLAMD_TRUNCATED_RESPONSE"));
        }
        if let Some(end) = chunk[..count].iter().position(|b| *b == 0) {
            if response.len() + end > 4096 {
                return Err(failure("CLAMD_RESPONSE_LIMIT"));
            }
            response.extend_from_slice(&chunk[..end]);
            let text = std::str::from_utf8(&response)
                .map_err(|_| failure("CLAMD_SCAN_RESPONSE"))?
                .trim();
            if text.is_empty() {
                return Err(failure("CLAMD_EMPTY_RESPONSE"));
            }
            return Ok(text.to_owned());
        }
        if response.len() + count > 4096 {
            return Err(failure("CLAMD_RESPONSE_LIMIT"));
        }
        response.extend_from_slice(&chunk[..count]);
    }
}
fn failure(code: &'static str) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        code,
        "The private malware scanner is unavailable.",
    )
}

impl crate::application::malware_scan::MalwareScanner for ClamAvScanner {
    async fn scan(
        &self,
        bytes: &[u8],
        clock: &(impl Clock + Sync),
    ) -> Result<MalwareScan, BusinessError> {
        ClamAvScanner::scan(self, bytes, clock).await
    }
}
