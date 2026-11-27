"use client";

import { useEffect, useState } from "react";
import { CalendarDays } from "lucide-react";
import { Button } from "@erpx/ui/components/button";
import { InlineNotice, Stack } from "@erpx/ui/components/foundation";
import { TextInput } from "@erpx/ui/components/input";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";

type Version = { id: string; version: string; repository: string };
type Installation = { id: string; installationVersion: number; manifest: { name: string; version: string } };
type SiteMeasurement = { leadId: string; subject: string; dueDate: string };
type Catalogue = { installed: Installation[]; versions: Version[]; canManage: boolean; siteMeasurement: SiteMeasurement | null };
type Command = { action: "install"; id: string; version: string } | { action: "withdraw"; id: string; installationVersion: number };
const github = "https://github.com";
const topicSearch = `${github}/search?q=topic%3Aerpx-plugin+archived%3Afalse&type=repositories`;

export function CompanyPluginCatalogue({ api, language }: { api: string; language: "en" | "th" }) {
  const th = language === "th";
  const [catalogue, setCatalogue] = useState<Catalogue | null>(null);
  const [requesting, setRequesting] = useState(false);
  const [message, setMessage] = useState("");
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState(false);

  async function load(signal?: AbortSignal) {
    const response = await workFetch(open ? api : `${api}?catalogue=0`, { signal });
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error?.message ?? "Unable to load plugins.");
    if (!signal?.aborted) setCatalogue(payload.data as Catalogue);
  }

  useEffect(() => {
    const request = new AbortController();
    void load(request.signal).catch(cause => { if (open && !request.signal.aborted) setMessage(cause.message); });
    return () => request.abort();
  }, [api, open]);

  async function submit(command: Command) {
    if (requesting) return;
    setRequesting(true);
    setMessage("");
    try {
      const response = await workFetch(api, {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(command),
      });
      const payload = await response.json();
      if (!response.ok) throw new Error(payload.error?.message ?? "Unable to update the installation.");
      await load();
    } catch (cause) {
      setMessage(cause instanceof Error ? cause.message : "Unable to update the installation.");
    } finally {
      setRequesting(false);
    }
  }

  const query = search.trim().toLowerCase();
  const pasted = /^https:\/\/github\.com\/([a-z0-9_.-]+\/[a-z0-9_.-]+?)\/?$/i.exec(query)?.[1]?.replace(/\.git$/, "");
  const versions = catalogue?.versions.filter(version => pasted
    ? pasted === version.repository
    : `${version.id} ${version.repository} ${version.version}`.includes(query));

  const siteMeasurement = catalogue?.siteMeasurement;
  let measurementHref: string | undefined;
  if (siteMeasurement && api.startsWith("/api/v1/holdings/")) {
    const base = api.slice("/api/v1".length, -"/plugins".length);
    const company = new URLSearchParams(window.location.search).get("company");
    const query = company ? `?${new URLSearchParams({ company })}` : "";
    measurementHref = `${base}/customers-sales/sales/pipeline/${encodeURIComponent(siteMeasurement.leadId)}/overview${query}#crm-activities`;
  }

  return <section aria-label={th ? "ปลั๊กอิน" : "Plugins"}>
    {measurementHref && siteMeasurement && <p>
      <a data-product-switch href={measurementHref}>
        <CalendarDays size={18} aria-hidden="true" /> {siteMeasurement.subject} · <time dateTime={siteMeasurement.dueDate}>{siteMeasurement.dueDate}</time>
      </a>
    </p>}
    <Button type="button" appearance="standard" variant="secondary" aria-expanded={open} onClick={() => setOpen(!open)}>
      {th ? "ปลั๊กอิน" : "Plugins"}
    </Button>
    {open && <Stack>
      <label>
        {th ? "ค้นหาหรือวางลิงก์ GitHub" : "Search or paste a GitHub repository link"}
        <TextInput appearance="standard" type="search" value={search} onChange={event => setSearch(event.target.value)} />
      </label>
      <a target="_blank" rel="noopener noreferrer" href={topicSearch}>{th ? "ค้นบน GitHub" : "Search on GitHub"}</a>
      {message && <InlineNotice tone="danger">{message}</InlineNotice>}
      {!catalogue && !message && <p role="status">{th ? "กำลังโหลดปลั๊กอิน" : "Loading plugins…"}</p>}
      {catalogue?.installed.map(item => <div key={item.id}>
        <strong>{item.manifest.name}</strong> · {item.manifest.version}
        {catalogue.canManage && <Button type="button" appearance="standard" variant="ghost" disabled={requesting}
          onClick={() => void submit({ action: "withdraw", id: item.id, installationVersion: item.installationVersion })}>
          {th ? "ถอนการติดตั้ง" : "Uninstall"}
        </Button>}
      </div>)}
      {versions?.map(version => <div key={`${version.id}@${version.version}`}>
        <strong>{version.id}</strong> · {version.version} · <a target="_blank" rel="noopener noreferrer" href={`${github}/${version.repository}`}>GitHub</a>
        <p>{th ? "อ่านนัดวัดหน้างานที่คุณเข้าใช้ได้ใน CRM" : "Reads site measurements you can access in CRM."}</p>
        {catalogue?.canManage && !catalogue.installed.some(item => item.id === version.id) && <Button type="button" appearance="standard" variant="ghost" disabled={requesting}
          onClick={() => void submit({ action: "install", id: version.id, version: version.version })}>
          {th ? "ติดตั้ง" : "Install"}
        </Button>}
      </div>)}
      {versions?.length === 0 && <p role="status">{th ? "ไม่พบปลั๊กอินที่อนุมัติ" : "No approved plugins match."}</p>}
    </Stack>}
  </section>;
}
