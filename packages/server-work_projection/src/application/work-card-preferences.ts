import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {WorkCardId,WorkCardPreferences} from "@erpx/feature-work_projection/lib/work-cards";
export type WorkCardPreferenceUnit={member(actor:WorkContext):Promise<boolean>;preference(actor:WorkContext):Promise<{pinned_ids_json:string|null;version:number}|null>;};
export function createWorkCardPreferenceService(ports:{error(status:number,code:string,message:string):Error}){
async function requireMember(db: WorkCardPreferenceUnit, actor: WorkContext) {
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
  db: WorkCardPreferenceUnit,
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
return {readWorkCardPreferences};}
