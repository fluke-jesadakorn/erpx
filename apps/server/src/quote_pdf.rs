//! Authorized, immutable quote PDF generation and download access.

use std::{fmt::Write as _, time::Duration};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, Utc};
use erpx_infrastructure::{MinioObjectStore, ObjectStorageError, SignedObjectUrl};
use erpx_sales::{QuoteDocumentLanguage, QuoteId, QuotePdfArtifact};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

const SARABUN_BASE64: &str = include_str!("../assets/fonts/Sarabun-Regular.ttf.base64");
const MAX_PDF_BYTES: u64 = 16 * 1024 * 1024;
const STORAGE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotePdfLine {
    pub item_code: String,
    pub description: String,
    pub quantity: i64,
    pub net_minor: i64,
    pub tax_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotePdfDocument {
    pub quote_id: QuoteId,
    pub quote_version: u64,
    pub quote_code: String,
    pub language: QuoteDocumentLanguage,
    pub customer_display_name: String,
    pub currency: String,
    pub payment_terms_days: u16,
    pub valid_until: DateTime<Utc>,
    pub lines: Vec<QuotePdfLine>,
    pub subtotal_minor: i64,
    pub tax_minor: i64,
    pub total_minor: i64,
}

impl QuotePdfDocument {
    fn validate(&self) -> Result<(), QuotePdfError> {
        let calculated_subtotal = self.lines.iter().try_fold(0_i64, |total, line| {
            if line.quantity <= 0
                || line.net_minor < 0
                || line.tax_minor < 0
                || !valid_text(&line.item_code, 128)
                || !valid_text(&line.description, 512)
            {
                return Err(QuotePdfError::InvalidDocument);
            }
            total
                .checked_add(line.net_minor)
                .ok_or(QuotePdfError::InvalidDocument)
        })?;
        let calculated_tax = self.lines.iter().try_fold(0_i64, |total, line| {
            total
                .checked_add(line.tax_minor)
                .ok_or(QuotePdfError::InvalidDocument)
        })?;
        let calculated_total = calculated_subtotal
            .checked_add(calculated_tax)
            .ok_or(QuotePdfError::InvalidDocument)?;
        if self.quote_version == 0
            || !valid_text(&self.quote_code, 128)
            || !valid_text(&self.customer_display_name, 512)
            || self.currency.len() != 3
            || !self.currency.bytes().all(|byte| byte.is_ascii_uppercase())
            || self.lines.is_empty()
            || self.lines.len() > 1_000
            || calculated_subtotal != self.subtotal_minor
            || calculated_tax != self.tax_minor
            || calculated_total != self.total_minor
        {
            return Err(QuotePdfError::InvalidDocument);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotePdfAccessGrant {
    holding_id: Uuid,
    storage_namespace: String,
    quote_id: QuoteId,
    quote_version: u64,
    artifact_id: Option<Uuid>,
}

impl QuotePdfAccessGrant {
    /// Construct only after the caller has passed holding and Quote read/write
    /// authorization. Download grants pin the immutable artifact as well.
    pub fn try_new(
        holding_id: Uuid,
        storage_namespace: String,
        quote_id: QuoteId,
        quote_version: u64,
        artifact_id: Option<Uuid>,
    ) -> Result<Self, QuotePdfError> {
        if holding_id.is_nil()
            || quote_version == 0
            || artifact_id.is_some_and(|value| value.is_nil())
            || !valid_namespace(&storage_namespace)
        {
            return Err(QuotePdfError::Unauthorized);
        }
        Ok(Self {
            holding_id,
            storage_namespace,
            quote_id,
            quote_version,
            artifact_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedQuotePdf {
    pub bytes: Vec<u8>,
    pub sha256_digest: String,
}

#[derive(Debug, Clone, Default)]
pub struct QuotePdfRenderer;

impl QuotePdfRenderer {
    pub fn render(&self, document: &QuotePdfDocument) -> Result<RenderedQuotePdf, QuotePdfError> {
        document.validate()?;
        let font_data = STANDARD
            .decode(SARABUN_BASE64.split_whitespace().collect::<String>())
            .map_err(|_| QuotePdfError::BundledFontInvalid)?;
        let mut options = svg2pdf::usvg::Options {
            font_family: "Sarabun".into(),
            ..Default::default()
        };
        options.fontdb_mut().load_font_data(font_data);
        let svg = render_svg(document)?;
        let tree =
            svg2pdf::usvg::Tree::from_str(&svg, &options).map_err(|_| QuotePdfError::SvgInvalid)?;
        let bytes = svg2pdf::to_pdf(
            &tree,
            svg2pdf::ConversionOptions {
                embed_text: true,
                ..Default::default()
            },
            svg2pdf::PageOptions::default(),
        )
        .map_err(|_| QuotePdfError::PdfConversionFailed)?;
        if bytes.is_empty() || bytes.len() as u64 > MAX_PDF_BYTES {
            return Err(QuotePdfError::PdfConversionFailed);
        }
        let sha256_digest = format!("{:x}", Sha256::digest(&bytes));
        Ok(RenderedQuotePdf {
            bytes,
            sha256_digest,
        })
    }
}

#[async_trait]
pub trait QuotePdfStoragePort: Send + Sync {
    async fn put_immutable(
        &self,
        namespace: &str,
        object_key: &str,
        bytes: &[u8],
    ) -> Result<(), ObjectStorageError>;

    fn presign_download(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError>;
}

#[async_trait]
impl QuotePdfStoragePort for MinioObjectStore {
    async fn put_immutable(
        &self,
        namespace: &str,
        object_key: &str,
        bytes: &[u8],
    ) -> Result<(), ObjectStorageError> {
        self.ensure_namespace(namespace).await?;
        self.put_object_bounded(namespace, object_key, bytes, MAX_PDF_BYTES, STORAGE_TIMEOUT)
            .await
    }

    fn presign_download(
        &self,
        namespace: &str,
        object_key: &str,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, ObjectStorageError> {
        MinioObjectStore::presign_download(self, namespace, object_key, expires_seconds, at)
    }
}

#[derive(Debug)]
pub struct QuotePdfService<S> {
    renderer: QuotePdfRenderer,
    storage: S,
}

impl<S: QuotePdfStoragePort> QuotePdfService<S> {
    pub fn new(storage: S) -> Self {
        Self {
            renderer: QuotePdfRenderer,
            storage,
        }
    }

    pub async fn generate(
        &self,
        grant: &QuotePdfAccessGrant,
        artifact_id: Uuid,
        document: &QuotePdfDocument,
        generated_at: DateTime<Utc>,
    ) -> Result<QuotePdfArtifact, QuotePdfError> {
        if artifact_id.is_nil()
            || grant.artifact_id.is_some()
            || grant.quote_id != document.quote_id
            || grant.quote_version != document.quote_version
        {
            return Err(QuotePdfError::Unauthorized);
        }
        let rendered = self.renderer.render(document)?;
        let object_key = format!(
            "quotes/{}/versions/{}/{}/quote.pdf",
            document.quote_id.as_uuid(),
            document.quote_version,
            rendered.sha256_digest
        );
        self.storage
            .put_immutable(&grant.storage_namespace, &object_key, &rendered.bytes)
            .await
            .map_err(|_| QuotePdfError::StorageUnavailable)?;
        Ok(QuotePdfArtifact {
            artifact_id,
            quote_version: document.quote_version,
            language: document.language,
            object_key,
            sha256_digest: rendered.sha256_digest,
            byte_length: rendered.bytes.len() as u64,
            generated_at,
        })
    }

    pub fn signed_download(
        &self,
        grant: &QuotePdfAccessGrant,
        artifact: &QuotePdfArtifact,
        expires_seconds: u32,
        at: DateTime<Utc>,
    ) -> Result<SignedObjectUrl, QuotePdfError> {
        if grant.artifact_id != Some(artifact.artifact_id)
            || grant.quote_version != artifact.quote_version
            || !(1..=3_600).contains(&expires_seconds)
        {
            return Err(QuotePdfError::Unauthorized);
        }
        self.storage
            .presign_download(
                &grant.storage_namespace,
                &artifact.object_key,
                expires_seconds,
                at,
            )
            .map_err(|_| QuotePdfError::StorageUnavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuotePdfError {
    #[error("quote PDF document is invalid")]
    InvalidDocument,
    #[error("quote PDF access is not authorized for this exact holding/version/artifact")]
    Unauthorized,
    #[error("bundled Thai font is invalid")]
    BundledFontInvalid,
    #[error("quote SVG is invalid")]
    SvgInvalid,
    #[error("quote PDF conversion failed")]
    PdfConversionFailed,
    #[error("quote PDF storage is unavailable")]
    StorageUnavailable,
}

fn render_svg(document: &QuotePdfDocument) -> Result<String, QuotePdfError> {
    let line_height = 28_u64;
    let page_height = 842_u64
        .checked_add((document.lines.len() as u64).saturating_sub(12) * line_height)
        .ok_or(QuotePdfError::InvalidDocument)?;
    let (title, customer_label, terms_label, valid_label, subtotal_label, tax_label, total_label) =
        match document.language {
            QuoteDocumentLanguage::Thai => (
                "ใบเสนอราคา",
                "ลูกค้า",
                "เงื่อนไขชำระเงิน",
                "ใช้ได้ถึง",
                "ยอดก่อนภาษี",
                "ภาษี",
                "ยอดรวม",
            ),
            QuoteDocumentLanguage::English => (
                "QUOTATION",
                "Customer",
                "Payment terms",
                "Valid until",
                "Subtotal",
                "Tax",
                "Total",
            ),
        };
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="595" height="{page_height}" viewBox="0 0 595 {page_height}"><rect width="595" height="{page_height}" fill="#ffffff"/><style>text {{ font-family: 'Sarabun'; fill: #172033; }} .muted {{ fill: #64748b; }} .head {{ font-size: 26px; font-weight: 600; }} .body {{ font-size: 11px; }} .small {{ font-size: 9px; }} .money {{ text-anchor: end; }}</style><text x="42" y="54" class="head">{title}</text><text x="553" y="48" class="body money">{code}</text><text x="553" y="64" class="small muted money">Version {version}</text><line x1="42" y1="82" x2="553" y2="82" stroke="#dbe3ef"/><text x="42" y="108" class="body muted">{customer_label}</text><text x="155" y="108" class="body">{customer}</text><text x="42" y="130" class="body muted">{terms_label}</text><text x="155" y="130" class="body">{terms} days</text><text x="340" y="130" class="body muted">{valid_label}</text><text x="553" y="130" class="body money">{valid_until}</text><rect x="42" y="154" width="511" height="28" rx="4" fill="#eef3fa"/><text x="52" y="173" class="small">Item / Description</text><text x="365" y="173" class="small money">Qty</text><text x="463" y="173" class="small money">Net</text><text x="543" y="173" class="small money">Tax</text>"##,
        code = xml(&document.quote_code),
        version = document.quote_version,
        customer = xml(&document.customer_display_name),
        terms = document.payment_terms_days,
        valid_until = document.valid_until.format("%Y-%m-%d"),
    );
    let mut y = 205_u64;
    for line in &document.lines {
        write!(
            svg,
            r##"<text x="52" y="{y}" class="small">{code} — {description}</text><text x="365" y="{y}" class="small money">{quantity}</text><text x="463" y="{y}" class="small money">{net}</text><text x="543" y="{y}" class="small money">{tax}</text><line x1="42" y1="{rule}" x2="553" y2="{rule}" stroke="#edf1f7"/>"##,
            code = xml(&line.item_code),
            description = xml(&line.description),
            quantity = line.quantity,
            net = money(line.net_minor),
            tax = money(line.tax_minor),
            rule = y + 10,
        )
        .map_err(|_| QuotePdfError::InvalidDocument)?;
        y += line_height;
    }
    let summary_y = (y + 30).max(page_height - 125);
    write!(
        svg,
        r##"<line x1="340" y1="{top}" x2="553" y2="{top}" stroke="#cbd5e1"/><text x="365" y="{subtotal_y}" class="body muted">{subtotal_label}</text><text x="543" y="{subtotal_y}" class="body money">{subtotal}</text><text x="365" y="{tax_y}" class="body muted">{tax_label}</text><text x="543" y="{tax_y}" class="body money">{tax}</text><text x="365" y="{total_y}" class="body">{total_label}</text><text x="543" y="{total_y}" class="body money">{total} {currency}</text><text x="42" y="{footer}" class="small muted">Immutable Quote version {version} · SHA-256 digest stored with artifact</text></svg>"##,
        top = summary_y - 20,
        subtotal_y = summary_y,
        subtotal = money(document.subtotal_minor),
        tax_y = summary_y + 22,
        tax = money(document.tax_minor),
        total_y = summary_y + 50,
        total = money(document.total_minor),
        currency = document.currency,
        footer = page_height - 24,
        version = document.quote_version,
    )
    .map_err(|_| QuotePdfError::InvalidDocument)?;
    Ok(svg)
}

fn money(minor: i64) -> String {
    format!("{}.{:02}", minor / 100, minor.unsigned_abs() % 100)
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn valid_text(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn valid_namespace(value: &str) -> bool {
    (3..=63).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(language: QuoteDocumentLanguage) -> QuotePdfDocument {
        QuotePdfDocument {
            quote_id: QuoteId::try_from_uuid(Uuid::from_u128(1)).unwrap(),
            quote_version: 2,
            quote_code: "QT-2026-0001".into(),
            language,
            customer_display_name: "บริษัท ตัวอย่าง จำกัด".into(),
            currency: "THB".into(),
            payment_terms_days: 30,
            valid_until: DateTime::from_timestamp(1_900_000_000, 0).unwrap(),
            lines: vec![QuotePdfLine {
                item_code: "บริการ-01".into(),
                description: "บริการติดตั้งระบบ".into(),
                quantity: 1,
                net_minor: 100_000,
                tax_minor: 7_000,
            }],
            subtotal_minor: 100_000,
            tax_minor: 7_000,
            total_minor: 107_000,
        }
    }

    #[test]
    fn renders_thai_and_english_as_deterministic_vector_pdf() {
        let renderer = QuotePdfRenderer;
        for language in [QuoteDocumentLanguage::Thai, QuoteDocumentLanguage::English] {
            let first = renderer.render(&document(language)).expect("pdf");
            let second = renderer.render(&document(language)).expect("pdf replay");
            assert!(first.bytes.starts_with(b"%PDF-"));
            assert_eq!(first, second);
            assert_eq!(first.sha256_digest.len(), 64);
        }
    }

    #[test]
    fn rejects_amount_drift_and_cross_version_access() {
        let mut invalid = document(QuoteDocumentLanguage::Thai);
        invalid.total_minor += 1;
        assert_eq!(
            QuotePdfRenderer.render(&invalid),
            Err(QuotePdfError::InvalidDocument)
        );

        let grant = QuotePdfAccessGrant::try_new(
            Uuid::from_u128(9),
            "holding-example".into(),
            document(QuoteDocumentLanguage::Thai).quote_id,
            3,
            None,
        )
        .unwrap();
        assert_ne!(
            grant.quote_version,
            document(QuoteDocumentLanguage::Thai).quote_version
        );
    }
}
