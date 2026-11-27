"use client";

import { useCallback, useEffect, useRef, useState, type MouseEvent } from "react";
import { ArrowUpRight, CalendarDays, Puzzle } from "lucide-react";
import { Button } from "@erpx/ui/components/button";
import { Dialog } from "@erpx/ui/components/dialog";
import { InlineNotice, Stack } from "@erpx/ui/components/foundation";
import { TextInput } from "@erpx/ui/components/input";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import styles from "./apps-launcher.module.css";

type Version = { id: string; version: string; repository: string };
type Installation = { id: string; installationVersion: number; manifest: {
  name: string; version: string; tile: { source: "crm.next-site-measurement.v1" };
} };
type SiteMeasurement = { leadId: string; subject: string; dueDate: string };
type Catalogue = { installed: Installation[]; versions: Version[]; canManage: boolean; siteMeasurement: SiteMeasurement | null };
type Command = { action: "install"; id: string; version: string } | { action: "withdraw"; id: string; installationVersion: number };
const github = "https://github.com";
const topicSearch = `${github}/search?q=topic%3Aerpx-plugin+archived%3Afalse&type=repositories`;

export function CompanyPluginCatalogue({ api, language, appQuery, onLaunch }: {
  api: string; language: "en" | "th"; appQuery: string; onLaunch: (event: MouseEvent<HTMLAnchorElement>) => void;
}) {
  const th = language === "th";
  const t = (thai: string, english: string) => th ? thai : english;
  const [catalogue, setCatalogue] = useState<Catalogue | null>(null);
  const [requesting, setRequesting] = useState(false);
  const [message, setMessage] = useState("");
  const [search, setSearch] = useState("");
  const [open, setOpen] = useState(false);
  const manageTrigger = useRef<HTMLButtonElement>(null);

  const load = useCallback(async (full: boolean, signal?: AbortSignal) => {
    const response = await workFetch(full ? api : `${api}?catalogue=0`, { signal });
    const payload = await response.json();
    if (!response.ok) throw new Error(payload.error?.message ?? "Unable to load plugins.");
    if (!signal?.aborted) setCatalogue(payload.data as Catalogue);
  }, [api]);

  useEffect(() => {
    const request = new AbortController();
    void load(open, request.signal).catch(cause => { if (open && !request.signal.aborted) setMessage(cause.message); });
    return () => request.abort();
  }, [load, open]);

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
      await load(true);
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
    const companyQuery = company ? `?${new URLSearchParams({ company })}` : "";
    measurementHref = `${base}/customers-sales/sales/pipeline/${encodeURIComponent(siteMeasurement.leadId)}/overview${companyQuery}#crm-activities`;
  }

  const filter = appQuery.trim().toLocaleLowerCase();
  const installed = catalogue?.installed.filter(item =>
    `${item.id} ${item.manifest.name} ${siteMeasurement?.subject ?? ""}`.toLocaleLowerCase().includes(filter)) ?? [];
  const manageLabel = t("จัดการปลั๊กอิน", "Manage add-ons");
  const showManage = catalogue?.canManage && `${manageLabel} plugin`.toLocaleLowerCase().includes(filter);

  return <>
    {(installed.length > 0 || showManage) && <section className={styles.plugins} aria-label={t("ปลั๊กอิน", "Add-ons")}>
      <h3>{t("ปลั๊กอิน", "Add-ons")}</h3>
      <div className={`${styles.grid} ${styles.pluginGrid}`}>
        {installed.map(item => {
          const measurement = item.manifest.tile.source === "crm.next-site-measurement.v1" ? siteMeasurement : null;
          const href = measurement ? measurementHref : undefined;
          const tile = <>
            <span className={styles.appIcon}><CalendarDays size={27} strokeWidth={1.65} aria-hidden="true" /></span>
            {href && <ArrowUpRight className={styles.openIcon} size={18} aria-hidden="true" />}
            <strong>{item.manifest.name}</strong>
            <small>{measurement ? <>{measurement.subject} · <time dateTime={measurement.dueDate}>{measurement.dueDate}</time></> :
              t("ยังไม่มีนัดวัดหน้างานที่กำลังจะมาถึง", "No upcoming site measurements")}</small>
          </>;
          return href
            ? <a className={`${styles.app} ${styles.pluginTile}`} key={item.id} data-plugin-id={item.id} data-product-switch href={href} onClick={onLaunch}>{tile}</a>
            : <article className={`${styles.app} ${styles.pluginTile}`} key={item.id} data-plugin-id={item.id}>{tile}</article>;
        })}
        {showManage && <button type="button" className={`${styles.app} ${styles.pluginTile}`} ref={manageTrigger} aria-haspopup="dialog" aria-expanded={open}
          onClick={() => { setMessage(""); setOpen(true); }}>
          <span className={styles.appIcon}><Puzzle size={27} strokeWidth={1.65} aria-hidden="true" /></span>
          <ArrowUpRight className={styles.openIcon} size={18} aria-hidden="true" />
          <strong>{manageLabel}</strong>
        </button>}
      </div>
    </section>}
    <Dialog open={open} onClose={() => setOpen(false)} title={manageLabel} appearance="standard" presentation="form-drawer" finalFocus={() => manageTrigger.current}>
      <Stack>
      <label>
        {th ? "ค้นหาหรือวางลิงก์ GitHub" : "Search or paste a GitHub repository link"}
        <TextInput appearance="standard" type="search" value={search} onChange={event => setSearch(event.target.value)} />
      </label>
      <a target="_blank" rel="noopener noreferrer" href={topicSearch}>{th ? "ค้นบน GitHub" : "Search on GitHub"}</a>
      {message && <InlineNotice tone="danger">{message}</InlineNotice>}
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
      {versions?.length === 0 && query && <p role="status">{th ? "ไม่พบปลั๊กอินที่อนุมัติ" : "No approved plugins match."}</p>}
      </Stack>
    </Dialog>
  </>;
}
