import type {QuickAccessPreference} from "@/lib/quick-access";
import type {Clock} from "../../shared/application/ports";
import {quickAccessPreferenceSchema,type SaveInput} from "../domain/quick-access-preferences";
export type QuickPreferenceUnit={transactional:boolean;preference(userId:string,surface:string,contextKey:string):Promise<{pinned_ids_json:string|null;version:number}|null>;receipt(key:string):Promise<{user_id:string;surface:string;context_key:string;request_hash:string;result_json:string}|null>;savePreference(userId:string,surface:string,contextKey:string,result:QuickAccessPreference,now:number):Promise<unknown>;saveReceipt(key:string,userId:string,surface:string,contextKey:string,hash:string,result:QuickAccessPreference,now:number):Promise<unknown>;transaction<Result>(action:(unit:QuickPreferenceUnit)=>Promise<Result>):Promise<Result>;};
export function createQuickPreferenceService(ports:{repository:QuickPreferenceUnit;clock:Clock;error(status:number,code:string,message:string):Error;hash(value:string):string}){
async function readFrom(
  db: QuickPreferenceUnit,
  userId: string,
  surface: string,
  contextKey: string,
): Promise<QuickAccessPreference> {
  const row = await db.preference(userId,surface,contextKey);
  return {
    pinnedIds: row?.pinned_ids_json === null || row === null
      ? null
      : JSON.parse(row.pinned_ids_json) as string[],
    version: row?.version ?? 0,
  };
}

async function readQuickAccessPreferences(
  userId: string,
  surface: string,
  contextKey: string,
): Promise<QuickAccessPreference> {
  return readFrom(ports.repository, userId, surface, contextKey);
}

async function saveQuickAccessPreferences(input: SaveInput): Promise<QuickAccessPreference> {
  const parsed = quickAccessPreferenceSchema.parse({
    pinnedIds: input.pinnedIds,
    expectedVersion: input.expectedVersion,
  });
  if (!input.userId || !input.surface || !input.contextKey)
    throw ports.error(422, "INVALID_SCOPE", "The shortcut scope is invalid.");
  if (!input.requestKey || input.requestKey.length > 200)
    throw ports.error(422, "IDEMPOTENCY_KEY_REQUIRED", "Provide a command key to save shortcuts.");
  const db = ports.repository;
  if (!db.transactional) throw new Error("CONTROL_TRANSACTION_REQUIRED");
  const hash = ports.hash(JSON.stringify([input.userId,input.surface,input.contextKey,parsed]));
  const available = new Set(input.availableIds);

  return db.transaction(async (tx) => {
    const prior = await tx.receipt(input.requestKey);
    if (prior) {
      if (prior.user_id !== input.userId || prior.surface !== input.surface ||
          prior.context_key !== input.contextKey || prior.request_hash !== hash)
        throw ports.error(409, "IDEMPOTENCY_CONFLICT", "This save key belongs to a different shortcut change.");
      return JSON.parse(prior.result_json) as QuickAccessPreference;
    }
    const current = await readFrom(tx, input.userId, input.surface, input.contextKey);
    if (current.version !== parsed.expectedVersion)
      throw ports.error(409, "VERSION_CONFLICT", "Your shortcuts changed on another device. Review the latest order before saving.");
    const added = (parsed.pinnedIds ?? []).filter((id) => !current.pinnedIds?.includes(id));
    if (added.some((id) => !available.has(id)))
      throw ports.error(403, "SHORTCUT_UNAVAILABLE", "A selected shortcut is no longer available.");
    const result: QuickAccessPreference = {
      pinnedIds: parsed.pinnedIds,
      version: current.version + 1,
    };
    await tx.savePreference(input.userId,input.surface,input.contextKey,result,ports.clock.now());
    await tx.saveReceipt(input.requestKey,input.userId,input.surface,input.contextKey,hash,result,ports.clock.now());
    return result;
  });
}

return {readQuickAccessPreferences,saveQuickAccessPreferences};}
