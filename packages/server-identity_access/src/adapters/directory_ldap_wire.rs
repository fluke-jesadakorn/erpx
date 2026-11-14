//! One owned TLS stream; no driver/background queues.
//! Upstream protocol values must never be logged or exposed to error Debug.
use super::directory_ber::{
    BerFailure, MAX_BER_DEPTH, MAX_BER_FRAME_BYTES, WireBudget, frame_length,
    validate_complete_frame,
};
use bytes::BytesMut;
use ldap3_proto::{LdapCodec, control::LdapControl, proto::*};
use std::{collections::BTreeSet, future::Future, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    time::{Instant, timeout_at},
};
use tokio_util::codec::{Decoder, Encoder};

const OPERATION_TIME: Duration = Duration::from_secs(15);
const SNAPSHOT_TIME: Duration = Duration::from_secs(60);
const MAX_PAGES: usize = 100;
const MAX_PAGE_COOKIE: usize = 4096;
const MAX_SEARCH_ROWS: usize = 10_000;
const MAX_REFERENCES: usize = 256;
const PAGE_SIZE: i64 = 500;
const MAX_RETAINED_ROW_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LdapFailure {
    Malformed,
    Incomplete,
    Limit,
    TimedOut,
    Transport,
    Credentials,
}
impl From<BerFailure> for LdapFailure {
    fn from(error: BerFailure) -> Self {
        match error {
            BerFailure::TooLarge
            | BerFailure::TooDeep
            | BerFailure::TooManyElements
            | BerFailure::Budget => Self::Limit,
            BerFailure::Truncated => Self::Incomplete,
            BerFailure::Indefinite | BerFailure::Malformed => Self::Malformed,
        }
    }
}

#[derive(Clone, Copy)]
pub enum ReferralPolicy {
    /// Only the original AD domain/container searches permit ignored referrals.
    /// The adapter never follows them or sends credentials to another endpoint.
    IgnoreActiveDirectoryPartition,
    Reject,
}

/// Only available to the owner adapter. No generic public plaintext constructor.
/// Production construction supplies a certificate-verified TLS stream; tests
/// supply explicitly synthetic duplex peers without operator credentials.
pub(super) struct SequentialLdap<S> {
    stream: S,
    codec: LdapCodec,
    budget: WireBudget,
    next_message: i32,
    deadline: Instant,
    poisoned: bool,
    retained_row_bytes: usize,
}
impl<S: AsyncRead + AsyncWrite + Unpin> SequentialLdap<S> {
    pub(super) fn new(stream: S, started: Instant) -> Self {
        Self {
            stream,
            codec: LdapCodec::new(Some(MAX_BER_FRAME_BYTES), Some(MAX_BER_DEPTH)),
            budget: WireBudget::new(),
            next_message: 1,
            deadline: started + SNAPSHOT_TIME,
            poisoned: false,
            retained_row_bytes: 0,
        }
    }
    fn operation_deadline(&self) -> Result<Instant, LdapFailure> {
        if self.poisoned {
            return Err(LdapFailure::Incomplete);
        }
        let now = Instant::now();
        if now >= self.deadline {
            return Err(LdapFailure::TimedOut);
        }
        Ok((now + OPERATION_TIME).min(self.deadline))
    }
    async fn before<T>(
        deadline: Instant,
        work: impl Future<Output = std::io::Result<T>>,
    ) -> Result<T, LdapFailure> {
        timeout_at(deadline, work)
            .await
            .map_err(|_| LdapFailure::TimedOut)?
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::UnexpectedEof {
                    LdapFailure::Incomplete
                } else {
                    LdapFailure::Transport
                }
            })
    }
    async fn send(
        &mut self,
        op: LdapOp,
        ctrl: Vec<LdapControl>,
        deadline: Instant,
    ) -> Result<i32, LdapFailure> {
        let id = self.next_message;
        self.next_message = id.checked_add(1).ok_or(LdapFailure::Limit)?;
        let mut output = BytesMut::new();
        // ldap3_proto emits raw parser errors at the tracing target. This scope
        // is synchronous (no await) and silences only upstream codec events.
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            self.codec.encode(
                LdapMsg {
                    msgid: id,
                    op,
                    ctrl,
                },
                &mut output,
            )
        })
        .map_err(|_| LdapFailure::Malformed)?;
        if output.len() > MAX_BER_FRAME_BYTES {
            return Err(LdapFailure::Limit);
        }
        self.budget.reserve(output.len())?;
        Self::before(deadline, self.stream.write_all(&output)).await?;
        Self::before(deadline, self.stream.flush()).await?;
        Ok(id)
    }
    async fn receive(&mut self, expected: i32, deadline: Instant) -> Result<LdapMsg, LdapFailure> {
        let mut header = [0u8; 6];
        Self::before(deadline, self.stream.read_exact(&mut header[..2])).await?;
        let count = if header[1] & 0x80 != 0 {
            usize::from(header[1] & 0x7f)
        } else {
            0
        };
        // This call rejects indefinite/count bombs before reading extra bytes.
        let first = frame_length(&header[..2])?;
        let (length, header_bytes) = if let Some(length) = first {
            (length, 2)
        } else {
            Self::before(deadline, self.stream.read_exact(&mut header[2..2 + count])).await?;
            (
                frame_length(&header[..2 + count])?.ok_or(LdapFailure::Malformed)?,
                2 + count,
            )
        };
        self.budget.reserve(length.total)?;
        // Declared size and cumulative budget are checked BEFORE body allocation.
        let mut bytes = BytesMut::with_capacity(length.total);
        bytes.extend_from_slice(&header[..header_bytes]);
        bytes.resize(length.total, 0);
        Self::before(deadline, self.stream.read_exact(&mut bytes[header_bytes..])).await?;
        validate_complete_frame(&bytes)?;
        let message =
            tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                self.codec.decode(&mut bytes)
            })
            .map_err(|_| LdapFailure::Malformed)?
            .ok_or(LdapFailure::Incomplete)?;
        if !bytes.is_empty() || expected <= 0 || message.msgid != expected {
            return Err(LdapFailure::Malformed);
        }
        Ok(message)
    }
    pub(super) async fn bind(&mut self, dn: String, password: String) -> Result<(), LdapFailure> {
        let deadline = self.operation_deadline()?;
        let result = async {
            if dn.is_empty() || dn.len() > 16_384 || password.is_empty() || password.len() > 65_536
            {
                return Err(LdapFailure::Credentials);
            }
            let id = self
                .send(
                    LdapOp::BindRequest(LdapBindRequest {
                        dn,
                        cred: LdapBindCred::Simple(password),
                    }),
                    vec![],
                    deadline,
                )
                .await?;
            let reply = self.receive(id, deadline).await?;
            if !reply.ctrl.is_empty() {
                return Err(LdapFailure::Malformed);
            }
            match reply.op {
                LdapOp::BindResponse(response)
                    if response.res.code == LdapResultCode::Success
                        && response.res.referral.is_empty()
                        && response.saslcreds.is_none() =>
                {
                    Ok(())
                }
                LdapOp::BindResponse(_) => Err(LdapFailure::Credentials),
                _ => Err(LdapFailure::Malformed),
            }
        }
        .await;
        self.poisoned |= result.is_err();
        result
    }

    /// The page collector returns only after every page has success+terminal
    /// cookie. A repeated/missing cookie, row limit, partial EOF or referral
    /// policy violation fails the entire observation; no partial Vec escapes.
    pub(super) async fn search(
        &mut self,
        request: LdapSearchRequest,
        paged: bool,
        referrals: ReferralPolicy,
        max_rows: usize,
    ) -> Result<Vec<LdapSearchResultEntry>, LdapFailure> {
        let result = self
            .search_complete(request, paged, referrals, max_rows)
            .await;
        self.poisoned |= result.is_err();
        result
    }
    async fn search_complete(
        &mut self,
        request: LdapSearchRequest,
        paged: bool,
        referrals: ReferralPolicy,
        max_rows: usize,
    ) -> Result<Vec<LdapSearchResultEntry>, LdapFailure> {
        if max_rows == 0
            || max_rows > MAX_SEARCH_ROWS
            || request.base.len() > 16_384
            || request.attrs.len() > 128
            || request.attrs.iter().any(|attribute| attribute.len() > 4096)
        {
            return Err(LdapFailure::Limit);
        }
        let mut rows = Vec::new();
        let mut cookie = Vec::new();
        let mut seen_cookies = BTreeSet::new();
        let mut references = 0usize;
        for _ in 0..MAX_PAGES {
            let deadline = self.operation_deadline()?;
            let controls = if paged {
                vec![LdapControl::SimplePagedResults {
                    size: PAGE_SIZE,
                    cookie: cookie.clone(),
                }]
            } else {
                vec![]
            };
            let id = self
                .send(LdapOp::SearchRequest(request.clone()), controls, deadline)
                .await?;
            loop {
                let reply = self.receive(id, deadline).await?;
                match reply.op {
                    LdapOp::SearchResultEntry(row) => {
                        if !reply.ctrl.is_empty() || rows.len() >= max_rows {
                            return Err(LdapFailure::Limit);
                        }
                        // Count decoded allocation before retaining a row. Wire
                        // byte limits alone do not bound many empty BER values.
                        self.retained_row_bytes = self
                            .retained_row_bytes
                            .checked_add(retained_size(&row)?)
                            .ok_or(LdapFailure::Limit)?;
                        if self.retained_row_bytes > MAX_RETAINED_ROW_BYTES {
                            return Err(LdapFailure::Limit);
                        }
                        rows.push(row);
                    }
                    LdapOp::SearchResultReference(_) => {
                        references += 1;
                        if !reply.ctrl.is_empty()
                            || references > MAX_REFERENCES
                            || matches!(referrals, ReferralPolicy::Reject)
                        {
                            return Err(LdapFailure::Incomplete);
                        }
                    }
                    LdapOp::SearchResultDone(done) => {
                        if done.code != LdapResultCode::Success || !done.referral.is_empty() {
                            return Err(LdapFailure::Incomplete);
                        }
                        let mut page_cookie = None;
                        for control in reply.ctrl {
                            match control {
                                LdapControl::SimplePagedResults { size, cookie: next }
                                    if paged
                                        && page_cookie.is_none()
                                        && size >= 0
                                        && next.len() <= MAX_PAGE_COOKIE =>
                                {
                                    page_cookie = Some(next)
                                }
                                LdapControl::Unknown {
                                    criticality: false, ..
                                } => {}
                                _ => return Err(LdapFailure::Malformed),
                            }
                        }
                        if !paged {
                            return Ok(rows);
                        }
                        cookie = page_cookie.ok_or(LdapFailure::Incomplete)?;
                        if cookie.is_empty() {
                            return Ok(rows);
                        }
                        if !seen_cookies.insert(cookie.clone()) {
                            return Err(LdapFailure::Incomplete);
                        }
                        break;
                    }
                    _ => return Err(LdapFailure::Malformed),
                }
            }
        }
        Err(LdapFailure::Limit)
    }
    pub(super) async fn close(mut self) {
        // No read after Unbind. Any earlier poison/error simply closes TLS;
        // this best-effort cleanup is never interpreted as snapshot success.
        if !self.poisoned
            && let Ok(deadline) = self.operation_deadline()
        {
            let _ = self.send(LdapOp::UnbindRequest, vec![], deadline).await;
        }
        let _ = timeout_at(
            Instant::now() + Duration::from_millis(100),
            self.stream.shutdown(),
        )
        .await;
    }
}

fn retained_size(row: &LdapSearchResultEntry) -> Result<usize, LdapFailure> {
    let mut bytes = std::mem::size_of::<LdapSearchResultEntry>()
        .saturating_add(row.dn.capacity())
        .saturating_add(
            row.attributes
                .capacity()
                .saturating_mul(std::mem::size_of::<LdapPartialAttribute>()),
        );
    for attribute in &row.attributes {
        bytes = bytes
            .saturating_add(attribute.atype.capacity())
            .saturating_add(
                attribute
                    .vals
                    .capacity()
                    .saturating_mul(std::mem::size_of::<Vec<u8>>()),
            );
        for value in &attribute.vals {
            bytes = bytes.saturating_add(value.capacity());
        }
    }
    if bytes > MAX_RETAINED_ROW_BYTES {
        return Err(LdapFailure::Limit);
    }
    Ok(bytes)
}
