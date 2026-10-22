import type { Metadata } from "next";
import { headers } from "next/headers";
import { frontAppForHost } from "@erpx/feature-shared/lib/front-host";
export async function generateMetadata():Promise<Metadata> {
  const app=frontAppForHost((await headers()).get("host"));
  return {title:app==="pms"?"PMS Front":"POS Front",manifest:"/manifest.webmanifest",icons:{icon:[{url:"/icon/192",sizes:"192x192",type:"image/png"},{url:"/icon/512",sizes:"512x512",type:"image/png"}],apple:"/icon/192"},appleWebApp:{capable:true,title:app==="pms"?"PMS Front":"POS Front",statusBarStyle:"default"}};
}
export default function FrontAccessLayout({children}:{children:React.ReactNode}) {return children;}
