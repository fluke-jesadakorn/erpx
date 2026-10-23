import {NextResponse} from 'next/server';
import {developmentConsoleRetired,consoleRetirementResult} from '@erpx/contracts/console-retirement';

export function legacyConsoleRetired(){return developmentConsoleRetired(process.env);}

/** Retained Next boundary shares the gateway's exact static development refusal. */
export function consoleRetirementResponse(request:Request):NextResponse|null {
 const result=consoleRetirementResult({host:request.headers.get('host'),method:request.method,url:request.url,accept:request.headers.get('accept')},process.env);
 return result?new NextResponse(result.body,{status:result.status,headers:result.headers}):null;
}
