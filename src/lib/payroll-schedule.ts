export type PayrollFrequency = "weekly"|"fortnightly"|"semimonthly"|"monthly";
export type PayrollSchedule = {frequency:PayrollFrequency; schedule_anchor_date:string|null; pay_delay_days:number};

const dayMs=86_400_000;
function dateNumber(value:string){const parsed=Date.parse(`${value}T00:00:00Z`);if(!Number.isFinite(parsed))throw new Error("INVALID_DATE");return Math.floor(parsed/dayMs);}
function fromDay(value:number){return new Date(value*dayMs).toISOString().slice(0,10);}
function addDays(value:string,days:number){return fromDay(dateNumber(value)+days);}

export function payrollPeriodForDate(group:PayrollSchedule,today:string){
  const year=Number(today.slice(0,4)),month=Number(today.slice(5,7)),day=Number(today.slice(8,10));
  let start:string,end:string;
  if(group.frequency==="weekly"||group.frequency==="fortnightly"){
    if(!group.schedule_anchor_date)throw new Error("PAYROLL_ANCHOR_REQUIRED");
    const length=group.frequency==="weekly"?7:14;
    const offset=Math.floor((dateNumber(today)-dateNumber(group.schedule_anchor_date))/length)*length;
    start=addDays(group.schedule_anchor_date,offset);end=addDays(start,length-1);
  }else if(group.frequency==="semimonthly"){
    start=`${today.slice(0,7)}-${day<=15?"01":"16"}`;
    end=day<=15?`${today.slice(0,7)}-15`:new Date(Date.UTC(year,month,0)).toISOString().slice(0,10);
  }else{
    start=`${today.slice(0,7)}-01`;
    end=new Date(Date.UTC(year,month,0)).toISOString().slice(0,10);
  }
  return {periodStart:start,periodEnd:end,payDate:addDays(end,group.pay_delay_days)};
}
