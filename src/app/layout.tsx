export const dynamic = "force-dynamic";
import { cookies } from "next/headers";
import type { Metadata } from "next";
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

export const metadata: Metadata = {
  title: "ERPX · Work connected",
  description: "A calm, connected workplace for enterprise operations.",
};

export default async function RootLayout({ children }: LayoutProps<"/">) {
  const preference = (await cookies()).get("erpx_appearance")?.value;
  const theme =
    preference === "dark" || preference === "system" ? preference : "light";
  return (
    <html
      lang="en"
      data-theme={theme}
      className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
    >
      <body>{children}</body>
    </html>
  );
}
