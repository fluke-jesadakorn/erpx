"use client";
import Link from "next/link";
import { useFrontLanguage } from "@/components/front/front-language";
import { frontText } from "@/lib/shared/front-language";
export type SetupItem = {
  id: string;
  label: string;
  ready: boolean;
  owner: string;
  href: string;
  detail?: string;
};
export function SetupChecklist({
  title = "Readiness",
  items,
}: {
  title?: string;
  items: SetupItem[];
}) {
  const language = useFrontLanguage(),
    tr = (value: string) => frontText(value, language);
  const missing = items.filter((item) => !item.ready).length;
  return (
    <section className="card setup-checklist" aria-label={tr(title)}>
      <header className="section-head">
        <h2>{tr(title)}</h2>
        <span className={`badge ${missing ? "warning" : "success"}`}>
          {missing
            ? language === "th"
              ? `เตรียม ${missing} รายการ`
              : `${missing} to prepare`
            : tr("Ready")}
        </span>
      </header>
      <ul>
        {items.map((item) => (
          <li key={item.id}>
            <span aria-hidden>{item.ready ? "✓" : "○"}</span>
            <div>
              <strong>{tr(item.label)}</strong>
              <p className="muted">
                {tr(item.owner)}
                {item.detail ? ` · ${tr(item.detail)}` : ""}
              </p>
            </div>
            <Link href={item.href}>{tr(item.ready ? "Review" : "Set up")}</Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
