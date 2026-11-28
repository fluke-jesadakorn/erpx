import 'server-only';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {ApiError} from '@erpx/platform-runtime-server/public/http';
import {sha256} from '@erpx/platform-runtime-server/public/crypto';
import {profileMediaFileOperations} from '@erpx/files-server/public/profile-media-storage';
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from '@erpx/workflow-server/public/commands';
import {BusinessRuleError} from '@erpx/platform-shared-server/index';
import {activeHoldingMember} from '@erpx/organization-server/adapters/member-profiles';
import {createProfileMediaCommandApplication} from '@erpx/organization-server/application/profile-media-command';
import { nativeOrgStatement } from './native-organization-owner-write';
import { memberProfileVersionGuard } from './organization-read-tail-sources-r51';
import type {ProfileMediaUpload, ProfileMediaRemoval, ProfileMediaResult} from '@erpx/organization-server/domain/profile-media-command';

function application(actor: WorkContext, readBody: (size: number) => Promise<ArrayBuffer>) {
  let write: SqlDatabase;
  const files = profileMediaFileOperations(actor);
  return createProfileMediaCommandApplication({actor, clock: {now: () => Date.now()}, identifiers: {next: () => crypto.randomUUID()},
    readyUpload: async () => { write = await activeHoldingMember(actor, actor.userId); await files.ready(); },
    readyRemoval: async () => { write = await activeHoldingMember(actor, actor.userId); },
    readyStoredRemoval: files.ready,
    readBody, digest: sha256, ...files,
    replayUpload: (key, input) => replayCommand<ProfileMediaResult>(write, actor, key, 'member.profile.media', actor.userId, input),
    replayRemoval: (key, input) => replayCommand<ProfileMediaResult>(write, actor, key, 'member.profile.media.remove', actor.userId, input),
    imageAtVersion: (kind, version) => write.prepare(`SELECT ${kind}_key AS key FROM member_profiles WHERE user_id=? AND version=?`).bind(actor.userId, version).first<{key: string | null}>(),
    commitUpload: async (commandId, input, objectKey, mediaType, nextVersion, now) => {
      const uploadKind = input.expectedVersion === 0
        ? (input.kind === 'avatar' ? 'profile_media_upload_avatar' : 'profile_media_upload_cover')
        : (input.kind === 'avatar' ? 'profile_media_update_avatar' : 'profile_media_update_cover');
      const statement = input.expectedVersion === 0
        ? await nativeOrgStatement(write, actor.companyId, uploadKind, [actor.userId, objectKey, mediaType, nextVersion, now], commandId, now)
        : await nativeOrgStatement(write, actor.companyId, uploadKind, [objectKey, mediaType, nextVersion, now, actor.userId, input.expectedVersion], commandId, now);
      return executeCommand(write, actor, commandId, 'member.profile.media', input, actor.userId, nextVersion, {version: nextVersion}, [
        assertion(write, commandId, 'profile-version', input.expectedVersion === 0 ? 'NOT EXISTS(SELECT 1 FROM member_profiles WHERE user_id=?)' : memberProfileVersionGuard(''), input.expectedVersion === 0 ? [actor.userId] : [actor.userId, input.expectedVersion]),
        statement,
      ]);
    },
    commitRemoval: async (commandId, input, objectKey, nextVersion, now) => {
      const removalKind = input.kind === 'avatar' ? 'profile_media_remove_avatar' : 'profile_media_remove_cover';
      const statement = await nativeOrgStatement(write, actor.companyId, removalKind, [nextVersion, now, actor.userId, input.version], commandId, now);
      return executeCommand(write, actor, commandId, 'member.profile.media.remove', input, actor.userId, nextVersion, {version: nextVersion}, [
        assertion(write, commandId, 'profile-version', memberProfileVersionGuard(` AND ${input.kind}_key=?`), [actor.userId, input.version, objectKey]),
        statement,
      ]);
    },
  });
}
async function exposed<T>(action: () => Promise<T>) { try { return await action(); } catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; } }
export const uploadMemberProfileMedia = (actor: WorkContext, input: ProfileMediaUpload, readBody: (size: number) => Promise<ArrayBuffer>) => exposed(() => application(actor, readBody).upload(input));
export const removeMemberProfileMedia = (actor: WorkContext, input: ProfileMediaRemoval) => exposed(() => application(actor, async () => {throw new Error('UPLOAD_BODY_NOT_AVAILABLE');}).remove(input));
