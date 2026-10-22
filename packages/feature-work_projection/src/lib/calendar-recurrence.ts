import { addDays } from "@erpx/server-people_payroll/public/rules/people-roster-domain";
import { localDateTimeToInstant } from "@erpx/feature-work_projection/lib/calendar-timeline";

export type RecurrenceRule={
  frequency:"daily"|"weekly"|"monthly"|"yearly";
  intervalCount:number;
  weekdays:number[];
  endKind:"never"|"count"|"until";
  occurrenceCount?:number|null;
  untilDate?:string|null;
};
export type CalendarOccurrence={key:string;startsAt:number;endsAt:number;startLocal:string;endLocal:string};
const dayNumber=(date:string)=>Math.round(Date.parse(`${date}T00:00:00Z`)/86400000);
const weekday=(date:string)=>((new Date(`${date}T00:00:00Z`).getUTCDay()+6)%7)+1;
function addLocalMinutes(local:string,minutes:number){
  return new Date(Date.parse(`${local}:00Z`)+minutes*60000).toISOString().slice(0,16);
}
function matches(date:string,start:string,rule:RecurrenceRule){
  const delta=dayNumber(date)-dayNumber(start);
  if(delta<0)return false;
  if(rule.frequency==="daily")return delta%rule.intervalCount===0;
  if(rule.frequency==="weekly")return Math.floor(delta/7)%rule.intervalCount===0&&
    (rule.weekdays.length?rule.weekdays.includes(weekday(date)):weekday(date)===weekday(start));
  const [year,month,day]=date.split("-").map(Number),[sy,sm,sd]=start.split("-").map(Number);
  if(rule.frequency==="monthly")return ((year-sy)*12+month-sm)%rule.intervalCount===0&&day===sd;
  return (year-sy)%rule.intervalCount===0&&month===sm&&day===sd;
}
/** Expand only the requested window. Invalid DST wall-clock occurrences are skipped. */
export function expandCalendarSeries(startLocal:string,endLocal:string,timezone:string,rule:RecurrenceRule,from:number,to:number,limit=500):CalendarOccurrence[]{
  const startDate=startLocal.slice(0,10);
  const duration=Math.round((Date.parse(`${endLocal}:00Z`)-Date.parse(`${startLocal}:00Z`))/60000);
  if(duration<=0||duration>31*1440)throw new Error("INVALID_RECURRENCE_DURATION");
  const stopDate=new Date(to+31*86400000).toISOString().slice(0,10);
  let date=startDate,ordinal=0,examined=0;
  const result:CalendarOccurrence[]=[];
  while(date<=stopDate&&examined++<400000){
    if(rule.endKind==="until"&&rule.untilDate&&date>rule.untilDate)break;
    if(matches(date,startDate,rule)){
      ordinal++;
      if(rule.endKind==="count"&&rule.occurrenceCount&&ordinal>rule.occurrenceCount)break;
      const key=`${date}${startLocal.slice(10)}`;
      const finish=addLocalMinutes(key,duration);
      try{
        const startsAt=localDateTimeToInstant(key,timezone),endsAt=localDateTimeToInstant(finish,timezone);
        if(endsAt>startsAt&&startsAt<to&&endsAt>from){
          result.push({key,startsAt,endsAt,startLocal:key,endLocal:finish});
          if(result.length>=limit)break;
        }
      }catch{/* A wall-clock time in the DST gap has no occurrence. */}
    }
    date=addDays(date,1);
  }
  return result;
}
