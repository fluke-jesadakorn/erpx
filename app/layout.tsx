import type { Metadata } from "next";
import "./globals.css";
import { GlobalLoading } from "@/components/ui/GlobalLoading";
import { LangGate } from "@/components/lang/LangGate";
import { IntlProvider } from "@/components/i18n/IntlProvider";
import { SecondaryLocaleProvider } from "@/components/i18n/SecondaryLocaleProvider";

export const dynamic = "force-dynamic";

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
    <html lang="th" suppressHydrationWarning>
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
      <body className="antialiased bg-slate-950 text-slate-50 min-h-screen font-sans">
        <SecondaryLocaleProvider>
          <IntlProvider>
            {children}
            <GlobalLoading />
            <LangGate />
          </IntlProvider>
        </SecondaryLocaleProvider>
      </body>
    </html>
  );
}