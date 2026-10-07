import {BusinessRuleError} from '../../shared';
export type ProfileMediaKind = 'avatar' | 'cover';
export type ProfileMediaResult = {version: number};
export type ProfileMediaUpload = {kind: ProfileMediaKind; contentType: string | null; contentLength: string | null; version: string | null; commandId: string | null};
export type ProfileMediaRemoval = {kind: ProfileMediaKind; version: string | null; commandId: string | null};
export function profileCommandKey(raw: string | null) {
  const key = raw ?? '';
  if (!/^[a-zA-Z0-9:_.-]{8,180}$/.test(key)) throw new BusinessRuleError(422, 'COMMAND_KEY_REQUIRED', 'A valid Idempotency-Key is required.');
  return key;
}
export function profileUploadMetadata(raw: ProfileMediaUpload) {
  const mediaType = raw.contentType?.split(';', 1)[0]?.trim().toLowerCase() ?? '';
  if (!['image/jpeg', 'image/png', 'image/webp'].includes(mediaType)) throw new BusinessRuleError(422, 'PROFILE_IMAGE_TYPE', 'Choose a JPEG, PNG or WebP image.');
  const max = raw.kind === 'avatar' ? 2 * 1024 * 1024 : 5 * 1024 * 1024, size = Number(raw.contentLength);
  if (!Number.isSafeInteger(size) || size < 1 || size > max) throw new BusinessRuleError(422, 'PROFILE_IMAGE_SIZE', `${raw.kind === 'avatar' ? 'Avatar' : 'Cover'} must be ${raw.kind === 'avatar' ? '2' : '5'} MB or smaller.`);
  const expectedVersion = Number(raw.version);
  if (!Number.isSafeInteger(expectedVersion) || expectedVersion < 0) throw new BusinessRuleError(422, 'PROFILE_VERSION_REQUIRED', 'Refresh your profile before uploading.');
  return {mediaType, size, expectedVersion, commandId: profileCommandKey(raw.commandId)};
}
export function profileRemovalMetadata(raw: ProfileMediaRemoval) {
  const version = Number(raw.version);
  if (!Number.isSafeInteger(version) || version < 1) throw new BusinessRuleError(422, 'PROFILE_VERSION_REQUIRED', 'Refresh your profile before removing an image.');
  return {version, commandId: profileCommandKey(raw.commandId)};
}
