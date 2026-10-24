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
