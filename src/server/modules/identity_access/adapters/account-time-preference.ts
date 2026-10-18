import 'server-only';
import {runtimeEnv} from '../../runtime/public/runtime';
import {ApiError} from '../../runtime/public/http';
import {BusinessRuleError} from '../../shared';
import {createAccountTimePreferenceApplication} from '../application/account-time-preference';
import type {TimePreference} from '../domain/account-time-preference';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {createHash} from 'node:crypto';
import {currentSession} from './authorization';
import {defaultTimePreference} from '../domain/account-time-preference';

function application() {
  let db: SqlDatabase;
  return createAccountTimePreferenceApplication({clock: {now: () => Date.now()}, ready: async () => {db = (await runtimeEnv()).CONTROL_DB;},
    read: id => db.prepare('SELECT mode,time_zone AS "timeZone",hour_cycle AS "hourCycle",version FROM account_time_preferences WHERE user_id=?').bind(id).first<TimePreference>(),
    create: (id, {mode, timeZone, hourCycle}, now) => db.prepare(`INSERT INTO account_time_preferences(user_id,mode,time_zone,hour_cycle,version,updated_at)
          VALUES(?,?,?,?,1,?) ON CONFLICT(user_id) DO NOTHING
          RETURNING mode,time_zone AS "timeZone",hour_cycle AS "hourCycle",version`).bind(id, mode, timeZone, hourCycle, now).first<TimePreference>(),
    change: (id, {mode, timeZone, hourCycle, version}, now) => db.prepare(`UPDATE account_time_preferences
          SET mode=?,time_zone=?,hour_cycle=?,version=version+1,updated_at=?
          WHERE user_id=? AND version=?
          RETURNING mode,time_zone AS "timeZone",hour_cycle AS "hourCycle",version`).bind(mode, timeZone, hourCycle, now, id, version).first<TimePreference>(),
  });
}
async function exposed<T>(action: () => Promise<T>) { try { return await action(); } catch (error) { if (error instanceof BusinessRuleError) throw new ApiError(error.status, error.code, error.message); throw error; } }
export const accountTimePreference = (userId: string) => exposed(() => application().read(userId));
export const changeAccountTimePreference = (userId: string, input: unknown) => exposed(() => application().change(userId, input));

/** Render-only preference seed. Never serialize a session or authority snapshot. */
export async function accountTimePreferenceSnapshot() {
  let key: string;
  let userId: string;
  try {
    const {session} = await currentSession();
    userId = session.user.id;
    key = createHash('sha256').update('time-preference:').update(userId).digest('hex');
  } catch (error) {
    if (error instanceof Error && error.message === 'UNAUTHENTICATED')
      return {key: 'anonymous', preference: defaultTimePreference};
    return null;
  }
  try { return {key, preference: await accountTimePreference(userId)}; }
  catch {
    // Keep the account identity during an optional preference outage. Never
    // discard unrelated drafts by remounting the application to retry a read.
    return {key, preference: undefined};
  }
}
