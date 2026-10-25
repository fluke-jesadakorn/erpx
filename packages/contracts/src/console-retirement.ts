export function developmentConsoleRetired(environment:Readonly<Record<string,string|undefined>>) {
 return environment.ERPX_CONSOLE_RETIRED==='enabled'&&environment.ERPX_ENVIRONMENT==='local'
  &&environment.ERPX_PRODUCT_ENVIRONMENT==='development'
  &&/^https:\/\/console-dev-erpx\.jesadakorn\.com\/?$/.test(environment.ERPX_PUBLIC_ORIGIN??'');
}

const headers = {
  "Cache-Control": "private, no-store",
  "CDN-Cache-Control": "no-store",
  "Cloudflare-CDN-Cache-Control": "no-store",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
  "X-Robots-Tag": "noindex, nofollow",
  "Content-Security-Policy": "default-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'",
};

/** Static refusal only. No request credential, query or body enters the response. */
export function consoleRetirementResult(request:{host:string|null|undefined;method:string;url:string;accept?:string|null},environment:Readonly<Record<string,string|undefined>>) {
 if(!developmentConsoleRetired(environment)||request.host!=='console-dev-erpx.jesadakorn.com')return null;
 const pathname=new URL(request.url,'http://retired.invalid').pathname;
 if((request.method==='GET'||request.method==='HEAD')&&(pathname==='/api/health'||pathname==='/api/ready'))return null;
 return {status:404,body:request.method==='HEAD'?null:'Not Found\n',headers:{...headers,'Content-Type':'text/plain; charset=utf-8'}};
}
