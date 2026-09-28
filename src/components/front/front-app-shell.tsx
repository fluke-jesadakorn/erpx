"use client";

import { useEffect, useState, useSyncExternalStore, type CSSProperties, type ReactNode } from "react";
import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import styles from "./front-app-shell.module.css";
import { FrontLanguageContext, useFrontLanguage } from "./front-language";
import { frontText } from "@/lib/front-language";
import { QuickAccessShelf } from "@/components/quick-access/shelf";
import type { QuickAccessItem, QuickAccessPreference } from "@/lib/quick-access";

type AppKind = "pos" | "pms";

function writeFrontCookie(name: string, value: string) {
  const secure = window.location.protocol === "https:" ? "; Secure" : "";
  document.cookie = name + "=" + encodeURIComponent(value) + "; Path=/; Max-Age=31536000; SameSite=Lax" + secure;
}

function subscribeToConnectivity(callback: () => void) {
  window.addEventListener("online", callback);
  window.addEventListener("offline", callback);
  return () => {
    window.removeEventListener("online", callback);
    window.removeEventListener("offline", callback);
  };
}
function getConnectivitySnapshot() { return navigator.onLine; }
function getServerConnectivitySnapshot() { return true; }

export function FrontAppShell({
  holdingSlug,
  companySlug,
  companyName,
  companyId,
  shortName,
  accentColor,
  logoAttachmentId,
  app,
  standalone = false,
  frontLocation,
  locationName,
  userId,
  initialLanguage,
  quickAccess,
  children,
}: {
  holdingSlug: string;
  companySlug: string;
  companyName: string;
  companyId: string;
  shortName: string;
  accentColor: string;
  logoAttachmentId: string | null;
  app: AppKind;
  standalone?: boolean;
  frontLocation?: string;
  locationName?: string;
  userId: string;
  initialLanguage: "th" | "en";
  quickAccess?: {
    items: QuickAccessItem[];
    preference: QuickAccessPreference;
    defaultIds: string[];
    endpoint: string;
  };
  children: ReactNode;
}) {
  const online = useSyncExternalStore(
    subscribeToConnectivity,
    getConnectivitySnapshot,
    getServerConnectivitySnapshot,
  );
  const [language, setLanguage] = useState(initialLanguage);
  const title = shortName + (app === "pos" ? " POS" : " PMS");
  const base = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug);
  const appHome = standalone ? "/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug) : base + "/" + app;
  const logo = base + "/" + app + "/logo";
  const style = { "--front-accent": accentColor } as CSSProperties;

  useEffect(() => {
    document.documentElement.lang = language;
    if (frontLocation) {
      const cookieName = ("erpx_front_" + app + "_" + companyId + "_" + userId).replace(/[^A-Za-z0-9_-]/g, "_");
      writeFrontCookie(cookieName, frontLocation);
      if (standalone) writeFrontCookie("erpx_front_device_target", appHome + "/" + frontLocation + "/");
    }
    return undefined;
  }, [app, appHome, companyId, frontLocation, language, standalone, userId]);

  function changeLanguage(next: "th" | "en") {
    setLanguage(next);
    writeFrontCookie("erpx_front_language_" + userId, next);
  }

  async function signOut() {
    const { authClient } = await import("@/lib/auth-client");
    await authClient.signOut();
    window.location.replace("/login");
  }

  return (
    <FrontLanguageContext.Provider value={language}>
    <div className={styles.app} data-front-app={app} data-front-location={frontLocation} style={style}>
      <a className={styles.skipLink} href="#front-content">{language === "th" ? "ข้ามไปยังเนื้อหา" : "Skip to page content"}</a>
      <header className={styles.header}>
        <a className={styles.brand} href={appHome + "/"} aria-label={title + " home"}>
          {logoAttachmentId ? <Image src={logo} alt="" width={128} height={128} unoptimized /> : <span className={styles.mark}>{Array.from(shortName.trim())[0]?.toLocaleUpperCase() ?? "•"}</span>}
          <span><strong>{title}</strong><small>{locationName ?? companyName}</small></span>
        </a>
        <div className={styles.controls}>
          <span className={online ? styles.online : styles.offline} role="status">{online ? (language === "th" ? "เชื่อมต่อแล้ว" : "Online") : (language === "th" ? "ออฟไลน์ · ส่งรายการไม่ได้" : "Offline · submissions paused")}</span>
          <button type="button" className={styles.language} onClick={() => changeLanguage(language === "th" ? "en" : "th")}>{language === "th" ? "EN" : "ไทย"}</button>
          {frontLocation && <Link className={styles.changeLocation} href={appHome + "/" + frontLocation + "/notifications"}>Notifications</Link>}
          <a className={styles.changeLocation} href={appHome + "/?locations=1"}>{language === "th" ? "เปลี่ยนสาขา" : "Change location"}</a>
          {standalone && <Link className={styles.changeLocation} href="/?choose=1">{language === "th" ? "เปลี่ยนบริษัท" : "Change company"}</Link>}
          <button type="button" className={styles.signOut} onClick={() => void signOut()}>{language === "th" ? "ออกจากระบบ" : "Sign out"}</button>
        </div>
      </header>
      {!online && <div className={styles.offlineBanner} role="alert">{language === "th" ? "ไม่มีการเชื่อมต่ออินเทอร์เน็ต · ข้อมูลยังไม่ได้ส่ง กรุณากลับมาเชื่อมต่อก่อนทำรายการ" : "Connection lost · data has not been sent. Reconnect before submitting."}</div>}
      <div className={`${styles.workArea} ${quickAccess ? styles.withQuickAccess : ""}`}>
        {quickAccess && <aside className={styles.quickAccess} aria-label={language === "th" ? "ทางลัด" : "Shortcuts"}>
          <QuickAccessShelf
            items={quickAccess.items.map((item) => ({ ...item, title: frontText(item.title, language) }))}
            initialPreferences={quickAccess.preference}
            defaultIds={quickAccess.defaultIds}
            endpoint={quickAccess.endpoint}
            title={language === "th" ? "เข้าถึงด่วน" : "Quick access"}
            requestMode="work"
            locale={language}
            placement="rail"
            embedded
          />
        </aside>}
        <main className={styles.content} id="front-content" tabIndex={-1}>{children}</main>
      </div>
    </div>
    </FrontLanguageContext.Provider>
  );
}

export function FrontLocationPicker({
  base,
  app,
  standalone = false,
  companyId,
  userId,
  locations,
  notice,
}: {
  base: string;
  app: AppKind;
  standalone?: boolean;
  companyId: string;
  userId: string;
  locations: Array<{ key: string; label: string; path: string }>;
  notice?: string;
}) {
  const router = useRouter();
  const language = useFrontLanguage();
  const cookieName = ("erpx_front_" + app + "_" + companyId + "_" + userId).replace(/[^A-Za-z0-9_-]/g, "_");
  function remember(path: string) {
    writeFrontCookie(cookieName, path);
    router.push(base + (standalone ? "" : "/" + app) + (path === "holding" ? "/holding/" : "/" + path.replace(/^\/+/, "") + "/"));
  }
  return (
    <section className={styles.locationCard}>
      <p className={styles.eyebrow}>{app === "pos" ? (language === "th" ? "แอปหน้าร้าน · POS" : "Store app · POS") : (language === "th" ? "แอปหน้าร้าน · PMS" : "Store app · PMS")}</p>
      <h1>{language === "th" ? "เลือกสาขาเพื่อเริ่มงาน" : "Choose a location to start"}</h1>
      <p>{language === "th" ? "รายการจะแสดงเฉพาะสถานที่ที่บัญชีของคุณได้รับสิทธิ์ และระบบจะตรวจสิทธิ์อีกครั้งก่อนเปิดงาน" : "Only locations your account can access are listed. Your access is checked again before the workspace opens."}</p>
      {notice && <p className={styles.notice} role="status">{notice}</p>}
      {locations.length ? <div className={styles.locations}>
        {locations.map((item) => <button type="button" key={item.key} onClick={() => remember(item.path)}>
          <span>{item.path === "holding" ? frontText("Company-wide", language) : item.label}</span><span aria-hidden="true">→</span>
        </button>)}
      </div> : <p className={styles.empty}>{language === "th" ? "ไม่พบสาขาหรือแผนกที่เปิดใช้ได้ ติดต่อผู้ดูแลบริษัท" : "No available branch or department. Contact your Company administrator."}</p>}
    </section>
  );
}

export function FrontPmsNavigation({ href, section, permissions }: { href: string; section: string; permissions?: string[] }) {
  const router = useRouter();
  const language = useFrontLanguage();
  const can = (permission: string) => permissions ? permissions.includes(permission) : true;
  const items = [
    ["dashboard", frontText("Overview", language), can("pms.read")],
    ["reservations", frontText("Reservations", language), can("pms.reservations")],
    ["front-desk", frontText("Front desk", language), can("pms.frontdesk")],
    ["rooms", frontText("Rooms", language), can("pms.housekeeping") || can("pms.maintenance")],
    ["housekeeping", frontText("Housekeeping", language), can("pms.housekeeping") || can("pms.inspect")],
    ["folios", frontText("Payments", language), can("pms.cashier")],
    ["cashier", frontText("Cashier", language), can("pms.cashier")],
    ["night-audit", frontText("Night audit", language), can("pms.audit")],
  ].filter((item) => item[2]) as Array<[string, string, boolean]>;
  return <nav className={styles.moduleNav} aria-label={language === "th" ? "งานโรงแรม" : "Hotel tasks"}>
    {items.map(([id, label]) => <button type="button" key={id} aria-current={section === id ? "page" : undefined} onClick={() => router.push(href + (id === "dashboard" ? "" : id))}>{label}</button>)}
  </nav>;
}
