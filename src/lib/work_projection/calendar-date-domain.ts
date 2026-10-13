/** Date formatting from a supplied clock instant. */
export function calendarTodayAt(now:Date,timeZone="Asia/Bangkok"){return new Intl.DateTimeFormat("en-CA",{timeZone,year:"numeric",month:"2-digit",day:"2-digit"}).format(now);}
