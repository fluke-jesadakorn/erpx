import { cookies } from "next/headers";
import { Suspense, type ReactNode } from "react";
import { PmsLanguageBoundary } from "@erpx/feature-pms/components/language-boundary";

export default async function PmsLayout({ children }: { children: ReactNode }) {
  const preference = (await cookies()).get("erpx_pms_language")?.value;
  const initialLanguage = preference === "th" ? "th" : "en";
  return (
    <Suspense fallback={children}>
      <PmsLanguageBoundary initialLanguage={initialLanguage}>
        {children}
      </PmsLanguageBoundary>
    </Suspense>
  );
}
