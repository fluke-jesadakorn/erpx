import {z} from "zod";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {WorkSnapshot} from "../domain/work-items";
import {dailyPlanSaveSchema,dailyPlanReferenceKey,currentItems,type DailyPlanView,type DailyPlanSave,type PlanRow,type StoredItem} from "../domain/daily-plan";
import type {Clock} from "../../shared/application/ports";
export type DailyPlanResult={date:string;version:number};
export type DailyPlanUnit<Effect>={read(companyId:string,userId:string,date:string):Promise<PlanRow|null>;lock(companyId:string,userId:string,date:string):Effect;version(key:string,companyId:string,userId:string,date:string,version:number):Effect;write(companyId:string,userId:string,date:string,items:string,version:number,key:string,now:number,expected:number):Effect;written(key:string,companyId:string,userId:string,date:string,version:number):Effect;replay(actor:WorkContext,key:string,id:string,input:DailyPlanSave):Promise<DailyPlanResult|null>;execute(actor:WorkContext,key:string,input:DailyPlanSave,id:string,result:DailyPlanResult,effects:Effect[]):Promise<DailyPlanResult>};
export function createDailyPlan<Effect>(ports:{clock:Clock;error(status:number,code:string,message:string):Error}){
/** The plan contains references only. Each read resolves current, authorized work. */
async function readDailyPlan(
  db: DailyPlanUnit<Effect>,
  actor: Pick<WorkContext, "companyId" | "userId">,
  date: string,
  snapshot: WorkSnapshot,
): Promise<DailyPlanView> {
  z.iso.date().parse(date);
  const row = await db.read(actor.companyId,actor.userId,date);
  const stored = row ? JSON.parse(row.items_json) as StoredItem[] : [];
  const visible = currentItems(snapshot);
  const items = stored.flatMap((item) => {
    const work = visible.get(dailyPlanReferenceKey(item));
    return work ? [{ ...item, sourceUpdatedAt: work.updatedAt, work }] : [];
  });
  const choices = [...visible.values()]
    .filter((work) => work.queue !== "completed");
  return {
    date,
    ownerKey: JSON.stringify([actor.companyId, actor.userId]),
    version: row?.version ?? 0,
    items,
    choices,
    hiddenCount: stored.length - items.length,
    sourcesUnavailable: snapshot.unavailableSources.length > 0,
  };
}

async function saveDailyPlan(
  db: DailyPlanUnit<Effect>,
  actor: WorkContext,
  raw: unknown,
  commandKey: string,
  snapshot: WorkSnapshot,
) {
  const input = dailyPlanSaveSchema.parse(raw);
  const recordId = `daily-plan:${actor.companyId}:${actor.userId}:${input.date}`;
  const prior = await db.replay(actor,commandKey,recordId,input);
  if (prior) return prior;
  const visible = currentItems(snapshot);
  for (const item of input.items) {
    const current = visible.get(dailyPlanReferenceKey(item));
    if (!current) throw ports.error(
      snapshot.unavailableSources.length ? 503 : 403,
      snapshot.unavailableSources.length ? "PLAN_SOURCE_UNAVAILABLE" : "PLAN_ITEM_UNAVAILABLE",
      snapshot.unavailableSources.length
        ? "A work source is temporarily unavailable. Try saving again when it returns."
        : "A planned work item is no longer available to you.",
    );
    if (current.updatedAt !== item.sourceUpdatedAt) throw ports.error(
      409, "PLAN_ITEM_CHANGED", "A planned work item changed. Reload the plan before saving.",
    );
  }
  const stored: StoredItem[] = input.items.map(({ type, id, variant, startMinute, plannedMinutes }) => ({
    type, id, variant, startMinute, plannedMinutes,
  }));
  const result = { date: input.date, version: input.expectedVersion + 1 };
  const now = ports.clock.now();
  return db.execute(actor,commandKey,input,recordId,result,[db.lock(actor.companyId,actor.userId,input.date),db.version(commandKey,actor.companyId,actor.userId,input.date,input.expectedVersion),db.write(actor.companyId,actor.userId,input.date,JSON.stringify(stored),result.version,commandKey,now,input.expectedVersion),db.written(commandKey,actor.companyId,actor.userId,input.date,result.version)]);
}

return {readDailyPlan,saveDailyPlan};}
