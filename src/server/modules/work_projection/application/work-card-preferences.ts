import type {WorkContext} from "../../identity_access/public/authorization";
import type {Clock} from "../../shared/application/ports";
import type {WorkCardId,WorkCardPreferences} from "@/lib/work-cards";
import type {WorkCardPreferenceInput} from "../domain/work-card-preferences";
export type WorkCardPreferenceUnit<Effect>={member(actor:WorkContext):Promise<boolean>;preference(actor:WorkContext):Promise<{pinned_ids_json:string|null;version:number}|null>;authority(companyId:string):Promise<(key:string)=>Effect[]>;versionGuard(key:string,actor:WorkContext,version:number):Effect;savePreference(actor:WorkContext,result:WorkCardPreferences,now:number):Effect;replayCommand<Result>(actor:WorkContext,key:string,operation:string,recordId:string,input:unknown):Promise<Result|null>;executeCommand<Result>(actor:WorkContext,key:string,operation:string,input:unknown,recordId:string,version:number,result:Result,effects:Effect[]):Promise<Result>;};
export function createWorkCardPreferenceService<Effect>(ports:{clock:Clock;error(status:number,code:string,message:string):Error}){
async function requireMember(db: WorkCardPreferenceUnit<Effect>, actor: WorkContext) {
  if (
    !(await db.member(actor))
  )
    throw ports.error(
      403,
      "MEMBERSHIP_REQUIRED",
      "Current workspace membership is required.",
    );
}
async function readWorkCardPreferences(
  db: WorkCardPreferenceUnit<Effect>,
  actor: WorkContext,
): Promise<WorkCardPreferences> {
  await requireMember(db, actor);
  const row = await db.preference(actor);
  return {
    pinnedIds: row?.pinned_ids_json
      ? (JSON.parse(row.pinned_ids_json) as WorkCardId[])
      : null,
    version: row?.version ?? 0,
  };
}
async function saveWorkCardPreferences(
  db: WorkCardPreferenceUnit<Effect>,
  actor: WorkContext,
  input: WorkCardPreferenceInput,
  key: string,
  available: () => Promise<WorkCardId[]>,
) {
  await requireMember(db, actor);
  const operation = "work.cards.preferences",
    recordId = `${actor.companyId}:${actor.userId}`;
  const replay = await db.replayCommand<WorkCardPreferences>(
    actor,
    key,
    operation,
    recordId,
    input,
  );
  if (replay) return replay;
  const current = await readWorkCardPreferences(db, actor);
  if (current.version !== input.expectedVersion)
    throw ports.error(
      409,
      "VERSION_CONFLICT",
      "Your shortcuts changed on another device. Review the latest arrangement before saving.",
    );
  const guard = await db.authority(actor.companyId);
  const added = (input.pinnedIds ?? []).filter(
    (id) => !current.pinnedIds?.includes(id),
  );
  if (added.length) {
    const allowed = await available();
    if (added.some((id) => !allowed.includes(id)))
      throw ports.error(
        403,
        "SHORTCUT_UNAVAILABLE",
        "A selected shortcut is unavailable in this workspace. Review your available actions.",
      );
  }
  const result: WorkCardPreferences = {
    pinnedIds: input.pinnedIds,
    version: current.version + 1,
  };
  return db.executeCommand(
    actor,
    key,
    operation,
    input,
    recordId,
    result.version,
    result,
    [
      ...guard(key),
      db.versionGuard(key,actor,input.expectedVersion),
      db.savePreference(actor,result,ports.clock.now()),
    ],
  );
}

return {readWorkCardPreferences,saveWorkCardPreferences};}
