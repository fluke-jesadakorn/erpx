"use client";

import { useState, type ReactNode } from "react";
import { useSearchParams } from "next/navigation";
import {
  FrontLanguageContext,
  FrontLocalizedTree,
  type FrontLanguage,
} from "@/components/front/front-language";
import { Field, Select } from "@/components/ui/foundation";

export function PmsLanguageBoundary({
  initialLanguage,
  children,
}: {
  initialLanguage: FrontLanguage;
  children: ReactNode;
}) {
  const [language, setLanguage] = useState(initialLanguage);
  const query = useSearchParams();
  const prototype = process.env.NODE_ENV === "development" &&
    (query.get("prototype") === "1" || ["A", "B", "C"].includes(query.get("variant") ?? ""));

  // The clickable prototype already owns its independent language control.
  if (prototype) return children;

  return (
    <FrontLanguageContext.Provider value={language}>
      <div lang={language}>
        <div className="ui-actions" style={{ justifyContent: "flex-end" }}>
          <Field appearance="standard">
            Language / ภาษา
            <Select
              appearance="standard"
              aria-label="Language / ภาษา"
              value={language}
              onChange={(event) => {
                const next: FrontLanguage = event.target.value === "th" ? "th" : "en";
                setLanguage(next);
                document.cookie = `erpx_pms_language=${next}; Path=/; Max-Age=31536000; SameSite=Lax`;
              }}
            >
              <option value="en">English</option>
              <option value="th">ไทย</option>
            </Select>
          </Field>
        </div>
        <FrontLocalizedTree>{children}</FrontLocalizedTree>
      </div>
    </FrontLanguageContext.Provider>
  );
}
