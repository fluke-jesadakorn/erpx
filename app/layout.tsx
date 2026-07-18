import type { Metadata } from "next";
import { Geist, Geist_Mono, Noto_Sans_Thai } from "next/font/google";
import "./globals.css";
import { GlobalLoading } from "@/components/ui/GlobalLoading";
import { LangGate } from "@/components/lang/LangGate";
import { IntlProvider } from "@/components/i18n/IntlProvider";
import { SecondaryLocaleProvider } from "@/components/i18n/SecondaryLocaleProvider";
import { ToastProvider } from "@/components/ui/Toast";

export const dynamic = "force-dynamic";

const geist = Geist({
  subsets: ["latin"],
  variable: "--font-geist-sans",
  display: "swap",
});

const geistMono = Geist_Mono({
  subsets: ["latin"],
  variable: "--font-geist-mono",
  display: "swap",
});

const notoThai = Noto_Sans_Thai({
  subsets: ["thai"],
  variable: "--font-noto-thai",
  display: "swap",
});

export const metadata: Metadata = {
  title: "Folio - AI Accounting & Finance Portal",
  description: "Manage receipts, semantic category mappings, and expense approvals with AI.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="th"
      className={`${geist.variable} ${geistMono.variable} ${notoThai.variable}`}
      suppressHydrationWarning
    >
      <head>
        <script
          dangerouslySetInnerHTML={{
            __html: [
              "document.documentElement.classList.add('gl-loading');",
              "try { var l = localStorage.getItem('folio.lang'); var lang = (l === 'de' || l === 'th') ? l : 'th'; document.documentElement.lang = lang; document.cookie = 'folio.locale=' + lang + '; Path=/; Max-Age=31536000; SameSite=Lax'; } catch (e) {}",
            ].join(' '),
          }}
        />
      </head>
      <body className="min-h-screen text-ink font-sans antialiased">
        <SecondaryLocaleProvider>
          <IntlProvider>
            <ToastProvider>
              {children}
              <GlobalLoading />
              <LangGate />
            </ToastProvider>
          </IntlProvider>
        </SecondaryLocaleProvider>
      </body>
    </html>
  );
}
