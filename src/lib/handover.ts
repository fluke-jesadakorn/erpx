import type { WorkItem } from "./work-item";

export type HandoverKind = "completed" | "pending" | "blocked";
export type HandoverItem = Pick<WorkItem,"id"|"title"|"status"|"updatedAt"|"href"|"nextAction"|"blocker"> & {
  kind:HandoverKind;sourceVersion:number;planned:boolean;
};
export function bangkokToday(now=Date.now()) {
  return new Date(now+7*60*60_000).toISOString().slice(0,10);
}
export function handoverKind(item:Pick<WorkItem,"queue"|"status"|"blocker">):HandoverKind {
  return item.blocker?.trim()?"blocked":item.queue==="completed"&&item.status==="completed"?"completed":"pending";
}
function oneLine(value:string,max=180) {
  return value.replace(/\s+/g," ").trim().slice(0,max);
}
/** Editable suggestion only: it never changes or closes any source work. */
export function handoverSuggestion(date:string,items:HandoverItem[]) {
  const label=new Intl.DateTimeFormat("en-GB-u-ca-gregory",{
    timeZone:"Asia/Bangkok",day:"numeric",month:"short",year:"numeric",
  }).format(new Date(`${date}T12:00:00Z`));
  const sections: string[]=[`Handover · ${label}`];
  for(const [kind,title] of [["completed","Completed"],["pending","Still in progress"],["blocked","Needs help"]] as const){
    const rows=items.filter(item=>item.kind===kind);
    if(!rows.length)continue;
    sections.push(title,...rows.map(item=>{
      const suffix=kind==="blocked"?oneLine(item.blocker??"Needs follow-up",140)
        :kind==="pending"?oneLine(item.nextAction??item.status,140):"";
      return `• ${oneLine(item.title,160)}${suffix?` — ${suffix}`:""}`;
    }));
  }
  sections.push("Next shift: ");
  return sections.join("\n").slice(0,3000);
}
