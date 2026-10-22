export * from "@erpx/server-people_payroll/public/rules/people-roster-domain";
import {rosterMonday as mondayForDate} from "@erpx/server-people_payroll/public/rules/people-roster-domain";
/** UI/native compatibility; roster rules accept an explicit date. */
export function rosterMonday(date=new Intl.DateTimeFormat("en-CA",{timeZone:"Asia/Bangkok",year:"numeric",month:"2-digit",day:"2-digit"}).format(new Date())){return mondayForDate(date);}
