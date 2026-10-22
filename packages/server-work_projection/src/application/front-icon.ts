import { storefrontAppIconSvg } from "@erpx/server-work_projection/domain/front-icon";
export type StorefrontIconInput = { shortName:string; accentColor:string; app:"pos"|"pms"; size:number; logo?:Uint8Array };
export function createStorefrontIconService(ports:{base64(bytes:Uint8Array):string;png(svg:string):Promise<Uint8Array<ArrayBuffer>>}) {
 return { storefrontAppIconPng(args:StorefrontIconInput) {return ports.png(storefrontAppIconSvg({...args,logo:args.logo?.byteLength?ports.base64(args.logo):undefined}));} };
}
