import s from "./legal.module.css";

function readable(key: string) {
  return key.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/[_.]/g, " ").replace(/^./, value => value.toUpperCase());
}

export function LegalEvidenceFields({ value }: { value: unknown }) {
  if (value === null || value === undefined) return <span className={s.small}>No evidence recorded.</span>;
  if (Array.isArray(value)) return <ul>{value.map((entry, index) => <li key={index}><LegalEvidenceFields value={entry} /></li>)}</ul>;
  if (typeof value === "object") return <dl className={s.evidenceFields}>{Object.entries(value as Record<string, unknown>).map(([key, entry]) => (
    <div key={key}><dt>{readable(key)}</dt><dd><LegalEvidenceFields value={entry} /></dd></div>
  ))}</dl>;
  return <span className={s.body}>{typeof value === "boolean" ? (value ? "Yes" : "No") : String(value)}</span>;
}

export function LegalSnapshot({ snapshot }: { snapshot: string }) {
  let value: unknown = snapshot;
  try { value = JSON.parse(snapshot); } catch {}
  return <LegalEvidenceFields value={value} />;
}
