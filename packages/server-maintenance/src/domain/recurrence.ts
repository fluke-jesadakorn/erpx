import {addDays} from "@erpx/server-people_payroll/public/rules/people-roster-domain";
import {localDateTimeCandidates,localMinuteAt} from "@erpx/feature-work_projection/lib/calendar-timeline";
import {BusinessRuleError} from "@erpx/server-shared/index";
export function wallClock(instant:number,zone:string){const local=localMinuteAt(instant,zone);return `${local.date}T${local.time}`;}
export function nextOccurrence(local:string,zone:string,days:number){
 const nextLocal=`${addDays(local.slice(0,10),days)}${local.slice(10)}`;
 // Keep the plan's wall-clock definition. A spring-forward gap moves only that
 // occurrence to the next valid minute; a fall-back fold selects its first instant.
 for(let minute=0;minute<=180;minute++){
  const resolved=minute?new Date(Date.parse(`${nextLocal}:00Z`)+minute*60000).toISOString().slice(0,16):nextLocal,candidates=localDateTimeCandidates(resolved,zone);
  if(candidates.length)return {local:nextLocal,at:candidates[0],resolvedLocal:resolved};
 }
 throw new BusinessRuleError(422,'MAINTENANCE_TIME_UNAVAILABLE','Choose a plan time with an available occurrence in this time zone.');
}
