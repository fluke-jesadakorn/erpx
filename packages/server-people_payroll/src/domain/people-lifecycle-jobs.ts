export type LifecycleEngagementFacts=Record<string,unknown>&{status:string;version:number;position_id:string|null;branch_id:string|null;department_id:string|null;level_id:string|null;manager_person_id:string|null;currency:string};export type LifecyclePositionFacts={id:string;is_ceo:boolean;level_id:string;branch_id:string|null;department_id:string|null;title:string};export function caseDate(value:number,timezone:string){
  const parts=new Intl.DateTimeFormat("en-US",{timeZone:timezone,year:"numeric",month:"2-digit",day:"2-digit"}).formatToParts(new Date(value));
  const get=(part:string)=>parts.find(item=>item.type===part)?.value??"";
  return `${get("year")}-${get("month")}-${get("day")}`;
}
export const placementFields = [
  "position_id", "branch_id", "department_id", "level_id",
  "manager_person_id", "job_title", "status", "start_date", "end_date",
] as const;
export function placementEngagementChanged(engagement:Record<string,unknown>,snapshot:Record<string,unknown>|null){
  const captured=snapshot?.engagementPlacement;
  // Cases scheduled before placement fields were recorded still fail closed on version drift.
  if(!captured || typeof captured!=="object" || Array.isArray(captured) ||
    !placementFields.every(field=>Object.hasOwn(captured,field)))
    return Number(engagement.version)!==Number(snapshot?.engagementVersion);
  const values=captured as Record<string,unknown>;
  return placementFields.some(field=>(engagement[field]??null)!==(values[field]??null));
}
