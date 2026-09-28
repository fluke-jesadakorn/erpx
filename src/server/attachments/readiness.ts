import "server-only";

import { attachmentUploadsFrozen } from "./feature";
import { attachmentPreviewReadiness } from "./preview";
import { attachmentScannerReadiness } from "./scanner";

/**
 * Compatibility upload adapters use the canonical quarantine lifecycle even
 * while the global UI gate is closed. Readiness therefore always probes clamd
 * with the scanner enabled instead of treating a disabled UI gate as healthy.
 *
 * Do not return local executable paths from this helper: `/api/health` is a
 * public operational endpoint and needs only the bounded readiness result.
 */
export async function attachmentRuntimeReadiness(environment: NodeJS.ProcessEnv = process.env) {
  const scannerEnvironment = { ...environment, ERPX_ATTACHMENTS_V2: "true" };
  const [scannerResult, previewResult] = await Promise.all([
    attachmentScannerReadiness(scannerEnvironment),
    attachmentPreviewReadiness(scannerEnvironment),
  ]);
  const expectedClamav=environment.ERPX_CLAMAV_VERSION??"1.5.4";
  const engineMatches=!scannerResult.ready||scannerResult.engine?.startsWith(`ClamAV ${expectedClamav}/`)===true;
  const scanner = {
    ...scannerResult,
    ready:scannerResult.ready&&engineMatches,
    error: !engineMatches ? "CLAMAV_ENGINE_VERSION_MISMATCH" : scannerResult.error && /^(?:CLAMD|CLAMAV|ERPX_CLAMD)_/.test(scannerResult.error)
      ? scannerResult.error
      : scannerResult.error ? "CLAMD_UNAVAILABLE" : null,
  };
  const preview = {
    ready: previewResult.ready,
    error: previewResult.error && /^ERPX_(?:HEIF|PDFTOPPM)_/.test(previewResult.error)
      ? previewResult.error
      : previewResult.error ? "ATTACHMENT_PREVIEW_TOOLS_UNAVAILABLE" : null,
  };
  return {
    ready: scanner.enabled && scanner.ready && preview.ready,
    scanner,
    preview,
    uploadFrozen: attachmentUploadsFrozen(environment),
  };
}
