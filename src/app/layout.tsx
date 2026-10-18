import {ProductLinkEnvironment} from "@/components/workplace/product-link-context";
import {productHostForHost} from "@/lib/runtime/product-hosts";
export const dynamic = "force-dynamic";
import { cookies } from "next/headers";
import { headers } from "next/headers";
import type { Metadata } from "next";
import { AttachmentFeatureProvider } from "@/components/files/attachment-feature-context";
import { TimePreferenceProvider } from "@/components/people_payroll/time/time-preference-context";
import { AccountPreferencesProvider } from "@/components/identity_access/account-preferences-provider";
import { ContextSwitchProvider } from "@/components/workplace/context-switch-provider";
import { marketingIsIndexable } from "@/lib/runtime/erpx-hosts";
import { appearancePreference } from "@/lib/shared/appearance-preference";
import { accountTimePreferenceSnapshot } from "@/server/modules/identity_access/public/account-time-preference";
import { accountPreferenceSnapshot } from "@/server/modules/identity_access/public/account-preference-snapshot";
import {
  attachmentsV2Enabled,
  attachmentsV2ReadOnly,
} from "@/server/modules/files/public/attachments/feature";
import { Geist } from "next/font/google";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

export async function generateMetadata(): Promise<Metadata> {
  const host = (await headers()).get("host");
  return {
    title: "ERPX · Work connected",
    description: "A calm, connected workplace for enterprise operations.",
    robots: marketingIsIndexable(host) ? { index: true, follow: true } : { index: false, follow: false },
  };
}

export default async function RootLayout({ children }: LayoutProps<"/">) {
  const cookieStore = await cookies(),audience=productHostForHost((await headers()).get("host"));
  const [timeSnapshot, accountSnapshot] = await Promise.all([accountTimePreferenceSnapshot(), accountPreferenceSnapshot()]);
  const fallback = { language: cookieStore.get("erpx_language")?.value === "th" ? "th" as const : "en" as const,
    appearance: appearancePreference(cookieStore.get("erpx_appearance")?.value), version: 0 };
  const snapshot = { key: accountSnapshot?.key ?? "unavailable", preference: accountSnapshot?.preference?.version
    ? accountSnapshot.preference : accountSnapshot?.preference ? fallback : undefined };
  const theme = snapshot.preference?.appearance ?? fallback.appearance;
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
    >
      <body>
        <AttachmentFeatureProvider appearance="standard" mode={attachmentFeatureMode}>
          <ProductLinkEnvironment audience={audience}><AccountPreferencesProvider appearance="standard" snapshot={snapshot}><ContextSwitchProvider appearance="standard"><TimePreferenceProvider appearance="standard" accountKey={timeSnapshot?.key ?? "unavailable"} initialPreference={timeSnapshot?.preference}>{children}</TimePreferenceProvider></ContextSwitchProvider></AccountPreferencesProvider></ProductLinkEnvironment>
        </AttachmentFeatureProvider>
      </body>
    </html>
  );
}
