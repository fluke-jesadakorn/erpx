export const dynamic = "force-dynamic";
import { cookies } from "next/headers";
import { headers } from "next/headers";
import type { Metadata } from "next";
import { AttachmentFeatureProvider } from "@/components/files/attachment-feature-context";
import { TimePreferenceProvider } from "@/components/people_payroll/time/time-preference-context";
import { ContextSwitchProvider } from "@/components/workplace/context-switch-provider";
import { marketingIsIndexable } from "@/lib/runtime/erpx-hosts";
import { appearancePreference } from "@/lib/shared/appearance-preference";
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
  const preference = (await cookies()).get("erpx_appearance")?.value;
  const theme = appearancePreference(preference);
  const attachmentFeatureMode = attachmentsV2Enabled()
    ? "enabled"
    : attachmentsV2ReadOnly()
      ? "read-only"
      : "disabled";
  return (
    <html
      lang="en"
      data-theme={theme}
      className={`${geistSans.variable} h-full antialiased`}
    >
      <body>
        <AttachmentFeatureProvider appearance="standard" mode={attachmentFeatureMode}>
          <ContextSwitchProvider appearance="standard"><TimePreferenceProvider appearance="standard">{children}</TimePreferenceProvider></ContextSwitchProvider>
        </AttachmentFeatureProvider>
      </body>
    </html>
  );
}
