/** The public request origin is independent of Next's loopback server URL. */
export function requestPublicOrigin(request: Request): string {
  const host = request.headers.get('host');
  for (const name of ['ERPX_FRONT_PMS_ORIGIN', 'ERPX_FRONT_POS_ORIGIN']) {
    const configuredFront = process.env[name];
    if (configuredFront && new URL(configuredFront).host === host) return new URL(configuredFront).origin;
  }
  const configured=process.env.ERPX_PUBLIC_ORIGIN;
  if(process.env.NODE_ENV==='production'){
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
