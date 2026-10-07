import {BusinessRuleError, type Clock, type IdentifierSource} from '../../shared';
import {profileUploadMetadata, profileRemovalMetadata, type ProfileMediaKind, type ProfileMediaUpload, type ProfileMediaRemoval, type ProfileMediaResult} from '../domain/profile-media-command';

export type ProfileUploadReceiptInput = {kind: ProfileMediaKind; expectedVersion: number; digest: string};
export type ProfileRemovalReceiptInput = {kind: ProfileMediaKind; version: number};
export interface ProfileMediaCommandPorts {
  actor: {holdingId: string; userId: string};
  clock: Clock;
  identifiers: IdentifierSource;
  readyUpload(): Promise<void>;
  readyRemoval(): Promise<void>;
  readyStoredRemoval(): Promise<void>;
  readBody(size: number): Promise<ArrayBuffer>;
  detectedMediaType(bytes: Uint8Array, declared: string): string | null;
  digest(bytes: ArrayBuffer): Promise<string>;
  scan(bytes: Uint8Array): Promise<{clean: boolean}>;
  preview(bytes: Uint8Array, declared: string): Promise<{bytes: Uint8Array; mediaType: string} | null>;
  store(key: string, bytes: Uint8Array, mediaType: string, digest: string): Promise<boolean>;
  removeStored(key: string): Promise<void>;
  replayUpload(key: string, input: ProfileUploadReceiptInput): Promise<ProfileMediaResult | null>;
  replayRemoval(key: string, input: ProfileRemovalReceiptInput): Promise<ProfileMediaResult | null>;
  imageAtVersion(kind: ProfileMediaKind, version: number): Promise<{key: string | null} | null>;
  commitUpload(key: string, input: ProfileUploadReceiptInput, objectKey: string, mediaType: string, nextVersion: number, now: number): Promise<ProfileMediaResult>;
  commitRemoval(key: string, input: ProfileRemovalReceiptInput, objectKey: string, nextVersion: number, now: number): Promise<ProfileMediaResult>;
}
export function createProfileMediaCommandApplication(ports: ProfileMediaCommandPorts) {
  return {
    async upload(raw: ProfileMediaUpload) {
      await ports.readyUpload();
      const {mediaType, size, expectedVersion, commandId} = profileUploadMetadata(raw);
      const buffer = await ports.readBody(size), bytes = new Uint8Array(buffer);
      if (ports.detectedMediaType(bytes, mediaType) !== mediaType) throw new BusinessRuleError(422, 'PROFILE_IMAGE_TYPE', 'Image contents do not match its type.');
      const digest = await ports.digest(buffer), input = {kind: raw.kind, expectedVersion, digest};
      const prior = await ports.replayUpload(commandId, input);
      if (prior) return prior;
      let scanned;
      try { scanned = await ports.scan(bytes); } catch { throw new BusinessRuleError(503, 'PROFILE_SCANNER_UNAVAILABLE', 'Image scanning is temporarily unavailable.'); }
      if (!scanned.clean) throw new BusinessRuleError(422, 'PROFILE_IMAGE_REJECTED', 'This image did not pass security scanning.');
      const preview = await ports.preview(bytes, mediaType);
      if (!preview) throw new BusinessRuleError(422, 'PROFILE_IMAGE_TYPE', 'Unable to process this image.');
      const objectKey = `${ports.actor.holdingId}/profiles/${ports.actor.userId}/${raw.kind}/${ports.identifiers.next()}.webp`;
      if (!await ports.store(objectKey, preview.bytes, preview.mediaType, digest)) throw new BusinessRuleError(503, 'PROFILE_IMAGE_STORAGE', 'Unable to store the image.');
      const nextVersion = expectedVersion + 1, now = ports.clock.now();
      try { return await ports.commitUpload(commandId, input, objectKey, preview.mediaType, nextVersion, now); }
      catch (error) { await ports.removeStored(objectKey).catch(() => undefined); throw error; }
    },
    async remove(raw: ProfileMediaRemoval) {
      await ports.readyRemoval();
      const {version, commandId} = profileRemovalMetadata(raw), input = {kind: raw.kind, version};
      const prior = await ports.replayRemoval(commandId, input);
      if (prior) return prior;
      const row = await ports.imageAtVersion(raw.kind, version);
      if (!row?.key) throw new BusinessRuleError(404, 'PROFILE_MEDIA_NOT_FOUND', 'Profile image unavailable.');
      const nextVersion = version + 1;
      const result = await ports.commitRemoval(commandId, input, row.key, nextVersion, ports.clock.now());
      await ports.readyStoredRemoval();
      await ports.removeStored(row.key).catch(() => undefined);
      return result;
    },
  };
}
