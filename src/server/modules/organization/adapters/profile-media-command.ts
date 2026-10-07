import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
import {sha256} from '../../runtime/public/crypto';
import {profileMediaFileOperations} from '../../files/public/profile-media-storage';
import {assertion, executeCommand, replayCommand} from '../../workflow/public/commands';
import {BusinessRuleError} from '../../shared';
import {activeHoldingMember} from './member-profiles';
import {createProfileMediaCommandApplication} from '../application/profile-media-command';
import type {ProfileMediaUpload, ProfileMediaRemoval, ProfileMediaResult} from '../domain/profile-media-command';

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
    commitUpload: (commandId, input, objectKey, mediaType, nextVersion, now) => executeCommand(write, actor, commandId, 'member.profile.media', input, actor.userId, nextVersion, {version: nextVersion}, [
      assertion(write, commandId, 'profile-version', input.expectedVersion === 0 ? 'NOT EXISTS(SELECT 1 FROM member_profiles WHERE user_id=?)' : 'EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=?)', input.expectedVersion === 0 ? [actor.userId] : [actor.userId, input.expectedVersion]),
      input.expectedVersion === 0 ? write.prepare(`INSERT INTO member_profiles(user_id,${input.kind}_key,${input.kind}_media_type,version,updated_at) VALUES(?,?,?,?,?)`).bind(actor.userId, objectKey, mediaType, nextVersion, now)
        : write.prepare(`UPDATE member_profiles SET ${input.kind}_key=?,${input.kind}_media_type=?,version=?,updated_at=? WHERE user_id=? AND version=?`).bind(objectKey, mediaType, nextVersion, now, actor.userId, input.expectedVersion),
    ]),
    commitRemoval: (commandId, input, objectKey, nextVersion, now) => executeCommand(write, actor, commandId, 'member.profile.media.remove', input, actor.userId, nextVersion, {version: nextVersion}, [
      assertion(write, commandId, 'profile-version', `EXISTS(SELECT 1 FROM member_profiles WHERE user_id=? AND version=? AND ${input.kind}_key=?)`, [actor.userId, input.version, objectKey]),
      write.prepare(`UPDATE member_profiles SET ${input.kind}_key=NULL,${input.kind}_media_type=NULL,version=?,updated_at=? WHERE user_id=? AND version=?`).bind(nextVersion, now, actor.userId, input.version),
    ]),
  });
}
async function exposed<T>(action: () => Promise<T>) { try { return await action(); } catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; } }
export const uploadMemberProfileMedia = (actor: WorkContext, input: ProfileMediaUpload, readBody: (size: number) => Promise<ArrayBuffer>) => exposed(() => application(actor, readBody).upload(input));
export const removeMemberProfileMedia = (actor: WorkContext, input: ProfileMediaRemoval) => exposed(() => application(actor, async () => {throw new Error('UPLOAD_BODY_NOT_AVAILABLE');}).remove(input));
