import 'server-only';
import type {FileStore} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {detectedAttachmentMediaType, sanitizedAttachmentPreview} from '@erpx/server-files/public/attachments/preview';
import {scanAttachmentBytes} from '@erpx/server-files/public/attachments/scanner';
import {attachmentsV2Enabled} from '@erpx/server-files/public/attachments/feature';
export function profileMediaFileOperations(actor: WorkContext) {
  let resource: FileStore | null = null;
  const files = async () => resource ??= (await holdingResources(actor.holdingId)).files;
  return {
    ready: async () => { await files(); },
    detectedMediaType: detectedAttachmentMediaType,
    scan: (bytes: Uint8Array) => scanAttachmentBytes(bytes, attachmentsV2Enabled() ? process.env : {...process.env, ERPX_ATTACHMENTS_V2: 'true'}),
    preview: sanitizedAttachmentPreview,
    store: async (key: string, bytes: Uint8Array, mediaType: string, digest: string) => !!await (await files()).put(key, bytes, {onlyIf: {etagDoesNotMatch: '*'}, httpMetadata: {contentType: mediaType, cacheControl: 'private, no-store'}, customMetadata: {sanitized: 'true', sourceSha256: digest}}),
    removeStored: async (key: string) => { await (await files()).delete(key); },
  };
}
