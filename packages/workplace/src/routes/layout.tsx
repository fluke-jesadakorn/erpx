import {requestProductOrigin} from '@erpx/feature-runtime/lib/product-hosts';
import {holdingRoutingEnabled,holdingHostForHost} from '@erpx/feature-runtime/lib/holding-hosts';
import type { LayoutProps } from "@erpx/contracts/next-routes";
import {ProductLinkEnvironment} from "@erpx/workplace/components/product-link-context";
import {appsEntryUrl} from "@erpx/feature-runtime/lib/application-origins";
import {productHostForHost} from "@erpx/feature-runtime/lib/product-hosts";
import {configuredProductOrigin,configuredProductHandoff} from '@erpx/feature-runtime/lib/product-hosts';
import {productIds} from '@erpx/contracts/product-registry';
import {createAuth} from '@erpx/server-identity_access/public/auth';
import {ProductAccountProvider} from '@erpx/workplace/components/product-account-provider';
export const dynamic = "force-dynamic";
import { cookies } from "next/headers";
import { headers } from "next/headers";
import type { Metadata } from "next";
import { AttachmentFeatureProvider } from "@erpx/feature-files/components/attachment-feature-context";
import { TimePreferenceProvider } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { AccountPreferencesProvider } from "@erpx/feature-identity_access/components/account-preferences-provider";
import { ContextSwitchProvider } from "@erpx/workplace/components/context-switch-provider";
import { marketingIsIndexable } from "@erpx/feature-runtime/lib/erpx-hosts";
import { appearancePreference } from "@erpx/feature-shared/lib/appearance-preference";
import { accountTimePreferenceSnapshot } from "@erpx/server-identity_access/public/account-time-preference";
import { accountPreferenceSnapshot } from "@erpx/server-identity_access/public/account-preference-snapshot";
import {
  attachmentsV2Enabled,
  attachmentsV2ReadOnly,
} from "@erpx/server-files/public/attachments/feature";
import { Geist } from "next/font/google";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

export async function generateMetadata(): Promise<Metadata> {
  const host = (await headers()).get("host");
  return {
    title: productHostForHost(host) === "apps" ? "ERPX Apps" : "ERPX · Work connected",
    description: "A calm, connected workplace for enterprise operations.",
    robots: marketingIsIndexable(host) ? { index: true, follow: true } : { index: false, follow: false },
  };
}

export default async function RootLayout({ children }: LayoutProps<"/">) {
  const cookieStore = await cookies(),audience=productHostForHost((await headers()).get("host"));
  const holding=holdingRoutingEnabled(process.env)?holdingHostForHost((await headers()).get('host')):undefined;
  const appsHref = audience === "apps" ? "/" : configuredProductHandoff("apps") ?? appsEntryUrl() ?? "/";
  const [timeSnapshot, accountSnapshot] = await Promise.all([accountTimePreferenceSnapshot(), accountPreferenceSnapshot()]);
  const fallback = { language: cookieStore.get("erpx_language")?.value === "th" ? "th" as const : "en" as const,
    appearance: appearancePreference(cookieStore.get("erpx_appearance")?.value), version: 0 };
  const snapshot = { key: accountSnapshot?.key ?? "unavailable", preference: accountSnapshot?.preference?.version
    ? accountSnapshot.preference : accountSnapshot?.preference ? fallback : undefined };
  const theme = snapshot.preference?.appearance ?? fallback.appearance;
  const staffAudience=audience&&['apps','admin',...productIds].includes(audience)?audience:null;
  const signedIn=staffAudience?await(await createAuth(requestProductOrigin(staffAudience,await headers())!)).api.getSession({headers:await headers()}):null;
  const timed=<TimePreferenceProvider appearance="standard" accountKey={timeSnapshot?.key ?? "unavailable"} initialPreference={timeSnapshot?.preference}>{children}</TimePreferenceProvider>;
  const accountContent=staffAudience&&signedIn?<ProductAccountProvider user={{id:signedIn.user.id,name:signedIn.user.name,email:signedIn.user.email}} audience={staffAudience} accountOrigin={configuredProductOrigin('account')!} appsHref={configuredProductHandoff('apps')!}>{timed}</ProductAccountProvider>:timed;
  const attachmentFeatureMode = attachmentsV2Enabled()
    ? "enabled"
    : attachmentsV2ReadOnly()
      ? "read-only"
      : "disabled";
  return (
    <html
      lang={snapshot.preference?.language ?? fallback.language}
      data-theme={theme}
      className={`${geistSans.variable} h-full antialiased`}
      data-erpx-app={holding?process.env.ERPX_APP_ID:undefined}
    >
      <body>
        <AttachmentFeatureProvider appearance="standard" mode={attachmentFeatureMode}>
          <ProductLinkEnvironment audience={audience} appsHref={appsHref} holdingSlug={holding?.holdingSlug} initialPathname={holding?(await headers()).get('x-erpx-visible-path')?.split('?')[0]:undefined}><AccountPreferencesProvider appearance="standard" snapshot={snapshot}><ContextSwitchProvider appearance="standard">{accountContent}</ContextSwitchProvider></AccountPreferencesProvider></ProductLinkEnvironment>
        </AttachmentFeatureProvider>
      </body>
    </html>
  );
}
