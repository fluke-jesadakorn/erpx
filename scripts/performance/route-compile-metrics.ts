const milliseconds=(value:string)=>{const match=/^(\d+(?:\.\d+)?)(ms|s)$/.exec(value);if(!match)throw new Error('COMPILE_DURATION_REQUIRED');return Number(match[1])*(match[2]==='s'?1000:1);};
/** The first actual route compilation is distinct from authenticated document
 * appearance, DB rendering and subsequent hot/cached requests. */
export function firstRouteCompile(log:string,route:string){
 const clean=log.replace(/\u001b\[[0-9;]*m/g,'');
 for(const line of clean.split('\n')){
  const request=/\bGET (\S+) (\d{3}) in ([\d.]+(?:ms|s)) \([^)]*\bcompile: ([\d.]+(?:ms|s))/.exec(line);
  if(!request||request[1].split('?')[0]!==route)continue;
  return{route,status:Number(request[2]),requestMs:milliseconds(request[3]),compileMs:milliseconds(request[4])};
 }
 throw new Error('FIRST_COLD_ROUTE_COMPILATION_NOT_OBSERVED');
}

export type RouteEnsureSpan={inputPage:string;timestamp:number;duration:number};
/** Next16.4 labels request internals `next.js`, which also includes framework
 * work. Its ensure-page trace is the actual lazy compiler/entry wait boundary,
 * in microseconds, before application rendering. Do not relabel request time. */
export function firstRouteEnsure(spans:RouteEnsureSpan[],route:string){
 const matches=(input:string)=>{
  const parts=input.replace(/\/page$/,'').split('/').filter(part=>part&&!/^\([^)]+\)$/.test(part));
  const pattern=parts.map(part=>/^\[\[\.\.\.[^\]]+\]\]$/.test(part)?'(?:/.*)?':/^\[\.\.\.[^\]]+\]$/.test(part)?'/.+':/^\[[^\]]+\]$/.test(part)?'/[^/]+':'/'+part.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')).join('');
  return new RegExp('^'+pattern+'/?$').test(route);
 };
 const sample=spans.filter(span=>Number.isFinite(span.duration)&&span.duration>=0&&Number.isFinite(span.timestamp)&&matches(span.inputPage)).sort((a,b)=>a.timestamp-b.timestamp)[0];
 if(!sample)throw new Error('COLD_ROUTE_ENSURE_PAGE_SPAN_REQUIRED');
 return{route,inputPage:sample.inputPage,ensurePageMs:sample.duration/1000,metric:'Next ensure-page lazy compilation/entry preparation; excludes application render'};
}
