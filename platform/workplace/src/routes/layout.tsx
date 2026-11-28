import {holdingRoutingEnabled,holdingHostForHost} from '@erpx/platform-runtime-ui/lib/holding-hosts';
import type { LayoutProps } from "@erpx/platform-contracts/next-routes";
import {ProductLinkEnvironment} from "@erpx/platform-workplace/components/product-link-context";
import {appsEntryUrl} from "@erpx/platform-runtime-ui/lib/application-origins";
import {productHostForHost} from "@erpx/platform-runtime-ui/lib/product-hosts";
import {configuredProductOrigin,configuredProductHandoff} from '@erpx/platform-runtime-ui/lib/product-hosts';
import {productIds} from '@erpx/platform-contracts/product-registry';
import { currentSession } from "@erpx/identity-access-server/public/authorization";
import {ProductAccountProvider} from '@erpx/platform-workplace/components/product-account-provider';
export const dynamic = "force-dynamic";
import { cookies } from "next/headers";
import { headers } from "next/headers";
import type { Metadata } from "next";
import { AttachmentFeatureProvider } from "@erpx/files-ui/components/attachment-feature-context";
import { TimePreferenceProvider } from "@erpx/people-payroll-ui/components/time/time-preference-context";
import { AccountPreferencesProvider } from "@erpx/identity-access-ui/components/account-preferences-provider";
import { ContextSwitchProvider } from "@erpx/platform-workplace/components/context-switch-provider";
import { marketingIsIndexable } from "@erpx/platform-runtime-ui/lib/erpx-hosts";
import { appearancePreference } from "@erpx/platform-shared-ui/lib/appearance-preference";
import { accountTimePreferenceSnapshot } from "@erpx/identity-access-server/public/account-time-preference";
import { accountPreferenceSnapshot } from "@erpx/identity-access-server/public/account-preference-snapshot";
import {
  attachmentsV2Enabled,
  attachmentsV2ReadOnly,
} from "@erpx/files-server/public/attachments/feature";
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
  const accountOrigin = configuredProductOrigin("account");
  const signedIn = staffAudience && accountOrigin
    ? await currentSession().catch((error: unknown) => {
        if (error instanceof Error && error.message === "UNAUTHENTICATED") return null;
        throw error;
      })
    : null;
  const accountUser = signedIn?.session.user;
  const timed=<TimePreferenceProvider appearance="standard" accountKey={timeSnapshot?.key ?? "unavailable"} initialPreference={timeSnapshot?.preference}>{children}</TimePreferenceProvider>;
  const accountContent = staffAudience && accountUser && accountOrigin
    ? (
      <ProductAccountProvider
        user={{ id: accountUser.id, name: accountUser.name, email: accountUser.email }}
        audience={staffAudience}
        accountOrigin={accountOrigin}
        appsHref={configuredProductHandoff("apps") ?? appsHref}
      >
        {timed}
      </ProductAccountProvider>
    )
    : timed;
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
