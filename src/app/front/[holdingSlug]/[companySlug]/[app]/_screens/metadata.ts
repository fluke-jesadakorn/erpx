import {headers} from 'next/headers';import type {Metadata} from 'next';import {frontAppForHost} from '@/lib/shared/front-host';import {frontCompanyBrand,type FrontAppKind} from '@/server/modules/work_projection/public/front-app';
export async function generateMetadata({
  params,
}: {
  params: Promise<{ holdingSlug: string; companySlug: string; app: string }>;
}): Promise<Metadata> {
  const { holdingSlug, companySlug, app: rawApp } = await params;
  if (rawApp !== "pos" && rawApp !== "pms") return {};
  const app = rawApp as FrontAppKind;
  const standalone = frontAppForHost((await headers()).get("host")) === app;
  const appBase = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) + "/" + app + "/";
  try {
    const entry = await frontCompanyBrand(holdingSlug, companySlug);
    if (!entry) return {};
    const title = entry.shortName + (app === "pos" ? " POS" : " PMS");
    return {
      title,
      applicationName: title,
      description: entry.companyName + (app === "pos" ? " point of sale" : " property management"),
      manifest: standalone ? "/manifest.webmanifest" : appBase + "manifest.webmanifest",
      icons: {
        icon: [
          { url: standalone ? "/icon/192" : appBase + "icon/192", sizes: "192x192", type: "image/png" },
          { url: standalone ? "/icon/512" : appBase + "icon/512", sizes: "512x512", type: "image/png" },
        ],
        apple: standalone ? "/icon/192" : appBase + "apple-icon.png",
      },
    };
  } catch (error) { throw error; }
}
