export const dynamic = "force-dynamic";
import { cookies } from "next/headers";
import { headers } from "next/headers";
import type { Metadata } from "next";
import { AttachmentFeatureProvider } from "@/components/attachments/attachment-feature-context";
import { TimePreferenceProvider } from "@/components/time/time-preference-context";
import { ContextSwitchProvider } from "@/components/workplace/context-switch-provider";
import { marketingIsIndexable } from "@/lib/erpx-hosts";
import { appearancePreference } from "@/lib/appearance-preference";
import {
  attachmentsV2Enabled,
  attachmentsV2ReadOnly,
} from "@/server/attachments/feature";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
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
      className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
    >
      <body>
        <AttachmentFeatureProvider mode={attachmentFeatureMode}>
          <ContextSwitchProvider><TimePreferenceProvider>{children}</TimePreferenceProvider></ContextSwitchProvider>
        </AttachmentFeatureProvider>
      </body>
    </html>
  );
}
