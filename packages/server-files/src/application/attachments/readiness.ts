import {createAttachmentFeatures,type AttachmentEnvironment} from "@erpx/server-files/domain/attachments/feature";
import type {AttachmentScannerReadiness} from "@erpx/server-files/public/attachments/scanner";
export function createAttachmentReadiness(ports:{scanner(environment:AttachmentEnvironment):Promise<AttachmentScannerReadiness>;preview(environment:AttachmentEnvironment):Promise<{ready:boolean;error:string|null}>}){
const features=createAttachmentFeatures(()=>new Error("unused gate error"));
/**
 * Compatibility upload adapters use the canonical quarantine lifecycle even
 * while the global UI gate is closed. Readiness therefore always probes clamd
 * with the scanner enabled instead of treating a disabled UI gate as healthy.
 *
 * Do not return local executable paths from this helper: `/api/health` is a
 * public operational endpoint and needs only the bounded readiness result.
 */
async function attachmentRuntimeReadiness(environment: AttachmentEnvironment) {
  const scannerEnvironment = { ...environment, ERPX_ATTACHMENTS_V2: "true" };
  const [scannerResult, previewResult] = await Promise.all([
    ports.scanner(scannerEnvironment),
    ports.preview(scannerEnvironment),
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
    uploadFrozen: features.attachmentUploadsFrozen(environment),
  };
}

return {attachmentRuntimeReadiness};}
