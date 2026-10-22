/** The public request origin is independent of Next's loopback server URL. */
export function publicOrigin(request:{host:string|null;url:string},configuration:{frontOrigins:(string|undefined)[];origin?:string;production:boolean}):string {
  const host = request.host;
  for (const configuredFront of configuration.frontOrigins) {
    if (configuredFront && new URL(configuredFront).host === host) return new URL(configuredFront).origin;
  }
  const configured=configuration.origin;
  if(configuration.production){
    if(!configured)throw new Error('PUBLIC_ORIGIN_REQUIRED');
    return new URL(configured).origin;
  }
  // Local browser/acceptance clients may use localhost or 127.0.0.1. Never
  // trust an arbitrary Host header to supply a production origin.
  if(host){
    const local=new URL('http://'+host);
    if(['localhost','127.0.0.1','[::1]'].includes(local.hostname))return local.origin;
  }
  return configured?new URL(configured).origin:new URL(request.url).origin;
}
