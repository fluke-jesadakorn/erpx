type ClosedPeriod = {id: string;start_date: string;end_date: string;state: string};

/** Suggest reviewed periods that cover the interval once, preferring the complete fiscal period. */
export function financeReportPeriods(periods: ClosedPeriod[], start: string, end: string): string[] {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(start) || !/^\d{4}-\d{2}-\d{2}$/.test(end) || start>end) return [];
  const candidates=periods.filter(p=>p.state==="closed"&&p.start_date>=start&&p.end_date<=end&&p.start_date<=p.end_date)
    .sort((a,b)=>b.end_date.localeCompare(a.end_date)||a.id.localeCompare(b.id));
  const missing=new Set<string>();
  function cover(cursor:string,depth:number):string[]|null {
    if(depth>=24)return null;
    const state=`${cursor}:${depth}`;
    if(missing.has(state))return null;
    for(const p of candidates.filter(p=>p.start_date===cursor)){
      if(p.end_date===end)return [p.id];
      const next=new Date(Date.parse(p.end_date)+86400000).toISOString().slice(0,10),remaining=cover(next,depth+1);
      if(remaining)return [p.id,...remaining];
    }
    missing.add(state);return null;
  }
  return cover(start,0)??[];
}
