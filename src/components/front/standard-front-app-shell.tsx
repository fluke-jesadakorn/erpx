"use client";
import {developmentProductOrigin} from "@/server/modules/commercial/public/product-registry";
import {ProductWorkLinks,useProductNavigationScope} from "@/components/workplace/product-link-context";
import {ProductNavigationBoundary} from "@/components/workplace/product-navigation-boundary";
import { MainContent } from "@/components/ui/main";
import { Badge, Button, Card, Field, InlineNotice, Select } from "@/components/ui/foundation";
import { PageHeader } from "@/components/ui/primitives";

import {
  useEffect,
  useState,
  useSyncExternalStore,
  type CSSProperties,
  type ReactNode,
} from "react";
import Image from "next/image";
import Link from "@/components/workplace/workspace-link";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@/lib/shared/destination-search";
import { useRouter } from "@/lib/shared/destination-router";
import styles from "./standard-front-app-shell.module.css";
import { FrontLanguageContext, useFrontLanguage } from "./front-language";
import { frontText } from "@/lib/shared/front-language";
import {
  CurrencyProvider,
  CurrencySelector,
  type CurrencyContextData,
} from "@/components/finance/money/currency-provider";
import { QuickAccessShelf } from "@/components/work_projection/quick-access/shelf";
import type { QuickAccessItem, QuickAccessPreference } from "@/lib/work_projection/quick-access";
import type { AppearancePreference } from "@/lib/shared/appearance-preference";
import { useContextSwitch } from "@/components/workplace/context-switch-provider";
import { useHydrated } from "@/components/ui/use-hydrated";
import { useAccountPreferences } from "@/components/identity_access/account-preferences-context";
import {
  confirmContextSwitch,
  contextSwitchBlockReason,
  setContextSwitching,
} from "@/lib/work_projection/context-switch";

type AppKind = "pos" | "pms";
type FrontWorkRole = {
  endpoint: string;
  assignmentId: string;
  revision: number;
  options: { id: string; label: string }[];
};

function FrontWorkRoleSelect({
  role,
  location,
  companyId,
  returnTo,
}: {
  role: FrontWorkRole;
  location: string;
  companyId: string;
  returnTo: string;
}) {
  const language = useFrontLanguage();
  const ready = useHydrated();
  const { busy, switchContext } = useContextSwitch();
  const [error, setError] = useState("");
  return (
    <div className={styles.workRole}>
      <Field appearance="standard">
        <span>{language === "th" ? "บทบาทงาน" : "Work role"}</span>
        <Select appearance="standard"
          aria-label={language === "th" ? "บทบาทงาน" : "Work role"}
          value={role.assignmentId}
          disabled={!ready || busy}
          onChange={async (event) => {
            const assignmentId = event.target.value;
            if (assignmentId === role.assignmentId || busy) return;
            setError("");
            try {
              await switchContext({
                companyId,
                returnTo,
                inspect: false,
                action: async () => {
                  const response = await fetch(role.endpoint, {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify({ location, assignmentId, revision: role.revision }),
                  });
                  if (!response.headers.get("content-type")?.includes("application/json")) {
                    throw new Error(
                      language === "th"
                        ? "ระบบยังไม่พร้อมเปลี่ยนบทบาท กรุณาลองใหม่อีกครั้ง"
                        : "Work role is temporarily unavailable. Please try again.",
                    );
                  }
                  const body = await response.json();
                  if (!response.ok)
                    throw new Error(
                      body.error?.message ??
                        (language === "th"
                          ? "เปลี่ยนบทบาทไม่สำเร็จ กรุณาลองใหม่"
                          : "Could not change work role. Try again."),
                    );
                },
              });
            } catch (cause) {
              setError(
                cause instanceof Error
                  ? cause.message
                  : language === "th"
                    ? "เปลี่ยนบทบาทไม่สำเร็จ กรุณาลองใหม่"
                    : "Could not change work role. Try again.",
              );
            }
          }}
        >
          {role.options.map((option) => (
            <option key={option.id} value={option.id}>
              {option.label}
            </option>
          ))}
        </Select>
      </Field>
      <small>
        {language === "th"
          ? "ใช้บทบาทที่เลือกทำรายการ กฎผู้ตรวจคนละคนยังคงใช้"
          : "Actions use this role. Independent review rules still apply."}
      </small>
      {error && <p role="alert">{error}</p>}
    </div>
  );
}

function writeFrontCookie(name: string, value: string) {
  const secure = window.location.protocol === "https:" ? "; Secure" : "";
  document.cookie =
    name + "=" + encodeURIComponent(value) + "; Path=/; Max-Age=31536000; SameSite=Lax" + secure;
}

function subscribeToConnectivity(callback: () => void) {
  window.addEventListener("online", callback);
  window.addEventListener("offline", callback);
  return () => {
    window.removeEventListener("online", callback);
    window.removeEventListener("offline", callback);
  };
}
function getConnectivitySnapshot() {
  return navigator.onLine;
}
function getServerConnectivitySnapshot() {
  return true;
}
function subscribeAppearance(callback: () => void) {
  window.addEventListener("erpx-appearance", callback);
  return () => window.removeEventListener("erpx-appearance", callback);
}
function getAppearanceSnapshot() {
  return document.documentElement.getAttribute("data-theme") ?? "system";
}

export function StandardFrontAppShell({
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
  initialTheme = "system",
  quickAccess,
  erpHref,
  money,
  moneyApiBase,
  workRole,
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
  initialTheme?: AppearancePreference;
  quickAccess?: {
    items: QuickAccessItem[];
    preference: QuickAccessPreference;
    defaultIds: string[];
    endpoint: string;
  };
  erpHref?: string;
  money?: CurrencyContextData;
  moneyApiBase?: string;
  workRole?: FrontWorkRole;
  children: ReactNode;
}) {
  const productScope=useProductNavigationScope();
  const pathname = usePathname(),
    search = useSearchParams();
  const online = useSyncExternalStore(
    subscribeToConnectivity,
    getConnectivitySnapshot,
    getServerConnectivitySnapshot,
  );
  const accountPreference = useAccountPreferences();
  const [localLanguage, setLanguage] = useState({ userId, value: initialLanguage });
  const [signOutError, setSignOutError] = useState("");
  const language = accountPreference?.preference && (accountPreference.preference.version > 0 || accountPreference.revision > 0)
    ? accountPreference.preference.language : localLanguage.userId === userId ? localLanguage.value : initialLanguage;
  const languageReady = useHydrated();
  const theme = useSyncExternalStore(
    subscribeAppearance,
    getAppearanceSnapshot,
    () => initialTheme,
  );
  const title = shortName + (app === "pos" ? " POS" : " PMS");
  const base = "/front/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug);
  const appHome = standalone
    ? "/" + encodeURIComponent(holdingSlug) + "/" + encodeURIComponent(companySlug)
    : base + "/" + app;
  const logo = base + "/" + app + "/logo";
  const style = { "--front-accent": accentColor } as CSSProperties;
  const canonicalFront = `/front/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(companySlug)}/${app}`;
  const returnPath = pathname.startsWith("/front/")
    ? pathname
    : `${canonicalFront}${pathname.slice(`/${encodeURIComponent(holdingSlug)}/${encodeURIComponent(companySlug)}`.length)}`;
  function sharedWork(path: string, hash = "") {
    const destination = new URL(`${erpHref!.replace(/\/$/, "")}/${path}`);
    destination.searchParams.set("frontReturn", `${returnPath}${search.size ? `?${search}` : ""}`);
    destination.hash = hash;
    return destination.toString();
  }

  useEffect(() => {
    document.documentElement.lang = language;
    if (frontLocation) {
      const cookieName = ("erpx_front_" + app + "_" + companyId + "_" + userId).replace(
        /[^A-Za-z0-9_-]/g,
        "_",
      );
      writeFrontCookie(cookieName, frontLocation);
      if (standalone)
        writeFrontCookie("erpx_front_device_target", appHome + "/" + frontLocation + "/");
    }
    return undefined;
  }, [app, appHome, companyId, frontLocation, language, standalone, userId]);

  function changeLanguage(next: "th" | "en") {
    setLanguage({ userId, value: next });
    writeFrontCookie("erpx_front_language_" + userId, next);
    window.dispatchEvent(new CustomEvent("erpx:account-language", { detail: { language: next, previousLanguage: language } }));
  }

  async function signOut() {
    const blockedReason = contextSwitchBlockReason();
    if (blockedReason) {
      setSignOutError(blockedReason);
      return;
    }
    const prompt = language === "th"
      ? "ออกจากระบบและทิ้งข้อมูลที่ยังไม่บันทึกหรือไม่?"
      : "Sign out and discard unsaved changes?";
    if (!confirmContextSwitch(prompt)) return;
    setSignOutError("");
    setContextSwitching(true);
    try {
      const { authClient } = await import("@/lib/identity_access/auth-client");
      await authClient.signOut();
      window.location.replace("/login");
    } catch (cause) {
      setContextSwitching(false);
      setSignOutError(cause instanceof Error ? cause.message : "Could not sign out. Try again.");
    }
  }

  const page = (
    <FrontLanguageContext.Provider value={language}>
      <div
        className={styles.app}
        data-front-app={app}
        data-front-location={frontLocation}
        style={style}
      >
        <a className={styles.skipLink} href="#front-content">
          {language === "th" ? "ข้ามไปยังเนื้อหา" : "Skip to page content"}
        </a>
        <header className={styles.header}>
          <a className={styles.brand} href={appHome + "/"} aria-label={title + " home"}>
            {logoAttachmentId ? (
              <Image src={logo} alt="" width={128} height={128} unoptimized />
            ) : (
              <span className={styles.mark}>
                {Array.from(shortName.trim())[0]?.toLocaleUpperCase() ?? "•"}
              </span>
            )}
            <span>
              <strong>{title}</strong>
              <small>{locationName ?? companyName}</small>
            </span>
          </a>
          <div className={styles.controls}>
            <Badge className={styles.connectivity} tone={online ? "success" : "danger"} role="status">
              {online
                ? language === "th"
                  ? "เชื่อมต่อแล้ว"
                  : "Online"
                : language === "th"
                  ? "ออฟไลน์ · ส่งรายการไม่ได้"
                  : "Offline · submissions paused"}
            </Badge>
            <Button appearance="standard"
              variant="plain"
              type="button"
              className={styles.language}
              disabled={!languageReady}
              data-client-managed-ready={String(languageReady)}
              onClick={() => changeLanguage(language === "th" ? "en" : "th")}
            >
              {language === "th" ? "EN" : "ไทย"}
            </Button>
            <Select appearance="standard"
              className={styles.language}
              aria-label={language === "th" ? "ธีมหน้าจอ" : "Appearance"}
              value={theme}
              onChange={(event) => {
                document.documentElement.setAttribute("data-theme", event.target.value);
                writeFrontCookie("erpx_appearance", event.target.value);
                window.dispatchEvent(new Event("erpx-appearance"));
              }}
            >
              <option value="light">{language === "th" ? "สว่าง" : "Light"}</option>
              <option value="dark">{language === "th" ? "มืด" : "Dark"}</option>
              <option value="system">{language === "th" ? "ตามระบบ" : "System"}</option>
            </Select>
            <details className={styles.accountMenu}>
              <summary>
                {language === "th" ? "เพิ่มเติม" : "More"}
                <span aria-hidden="true"> ···</span>
              </summary>
              <div className={styles.accountActions}>
                {workRole && frontLocation && workRole.options.length > 1 && (
                  <FrontWorkRoleSelect
                    role={workRole}
                    location={frontLocation}
                    companyId={companyId}
                    returnTo={appHome + "/" + frontLocation}
                  />
                )}
                {money && <CurrencySelector appearance="standard" />}
                {productScope.audience === app ? <>
                  {erpHref && frontLocation && <Link className={styles.changeLocation} href={sharedWork(app)}>
                    {language === "th" ? "จัดการและตั้งค่า" : "Back office & setup"}
                  </Link>}
                  <a data-product-switch className={styles.changeLocation} href={developmentProductOrigin("apps") + "/sso/start"}>Apps</a>
                </> : erpHref && (
                  <a className={styles.changeLocation} href={erpHref}>
                    {language === "th" ? "← กลับ ERP" : "← Back to ERP"}
                  </a>
                )}
                {erpHref && frontLocation && (
                  <nav aria-label="Shared work" className={styles.sharedWork}>
                    <Link href={sharedWork("home")}>Today</Link>
                    <Link href={sharedWork("work")}>My work</Link>
                    <Link href={sharedWork("inbox")}>Messages</Link>
                    {productScope.audience===app&&<Link href={sharedWork("inbox/notifications")}>{language==="th"?"การแจ้งเตือนงาน":"Work updates"}</Link>}
                    <Link href={sharedWork("home", "handover")}>Handover</Link>
                  </nav>
                )}
                {frontLocation && (
                  <Link
                    className={styles.changeLocation}
                    href={appHome + "/" + frontLocation + "/notifications"}
                  >
                    {language === "th" ? "การแจ้งเตือน" : "Notifications"}
                  </Link>
                )}
                <a className={styles.changeLocation} href={appHome + "/?locations=1"}>
                  {language === "th" ? "เปลี่ยนสาขา" : "Change location"}
                </a>
                {standalone && (
                  <Link className={styles.changeLocation} href="/?choose=1">
                    {language === "th" ? "เปลี่ยนบริษัท" : "Change company"}
                  </Link>
                )}
                {signOutError && <p role="alert">{signOutError}</p>}
                <Button appearance="standard"
                  variant="plain"
                  type="button"
                  className={styles.signOut}
                  onClick={() => void signOut()}
                >
                  {language === "th" ? "ออกจากระบบ" : "Sign out"}
                </Button>
              </div>
            </details>
          </div>
        </header>
        {!online && (
          <InlineNotice tone="danger" className={styles.offlineBanner} role="alert">
            {language === "th"
              ? "ไม่มีการเชื่อมต่ออินเทอร์เน็ต · ข้อมูลยังไม่ได้ส่ง กรุณากลับมาเชื่อมต่อก่อนทำรายการ"
              : "Connection lost · data has not been sent. Reconnect before submitting."}
          </InlineNotice>
        )}
        <div className={`${styles.workArea} ${quickAccess ? styles.withQuickAccess : ""}`}>
          {quickAccess && (
            <aside
              className={styles.quickAccess}
              aria-label={language === "th" ? "ทางลัด" : "Shortcuts"}
            >
              <QuickAccessShelf
                items={quickAccess.items.map((item) => ({
                  ...item,
                  title: frontText(item.title, language),
                }))}
                initialPreferences={quickAccess.preference}
                defaultIds={quickAccess.defaultIds}
                endpoint={quickAccess.endpoint}
                title={language === "th" ? "เข้าถึงด่วน" : "Quick access"}
                requestMode="work"
                locale={language}
                placement="rail"
                embedded
              />
            </aside>
          )}
          <MainContent className={styles.content} id="front-content" tabIndex={-1}>
            {children}
          </MainContent>
        </div>
      </div>
    </FrontLanguageContext.Provider>
  );
  const content = money && moneyApiBase ? (
    <CurrencyProvider initial={money} apiBase={moneyApiBase}>
      {page}
    </CurrencyProvider>
  ) : (
    page
  );
  const linked=erpHref?<ProductWorkLinks companyId={companyId} companySlug={companySlug} workspaceBase={new URL(erpHref,"https://front.invalid").pathname}>{content}</ProductWorkLinks>:content;
  return productScope.audience===app?<ProductNavigationBoundary appearance="standard" language={language}>{linked}</ProductNavigationBoundary>:linked;
}

export function StandardFrontLocationPicker({
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
  const cookieName = ("erpx_front_" + app + "_" + companyId + "_" + userId).replace(
    /[^A-Za-z0-9_-]/g,
    "_",
  );
  function remember(path: string) {
    writeFrontCookie(cookieName, path);
    router.push(
      base +
        (standalone ? "" : "/" + app) +
        (path === "holding" ? "/holding/" : "/" + path.replace(/^\/+/, "") + "/"),
    );
  }
  return (
    <Card appearance="standard" className={styles.locationCard}>
      <PageHeader appearance="standard" eyebrow={app === "pos"
          ? language === "th" ? "แอปหน้าร้าน · POS" : "Store app · POS"
          : language === "th" ? "แอปหน้าร้าน · PMS" : "Store app · PMS"}
        title={language === "th" ? "เลือกสาขาเพื่อเริ่มงาน" : "Choose a location to start"}
        description={language === "th"
          ? "รายการจะแสดงเฉพาะสถานที่ที่บัญชีของคุณได้รับสิทธิ์ และระบบจะตรวจสิทธิ์อีกครั้งก่อนเปิดงาน"
          : "Only locations your account can access are listed. Your access is checked again before the workspace opens."}/>
      {notice && (
        <InlineNotice tone="warning" role="status">
          {notice}
        </InlineNotice>
      )}
      {locations.length ? (
        <div className={styles.locations}>
          {locations.map((item) => (
            <Button appearance="standard"
              variant="plain"
              type="button"
              key={item.key}
              onClick={() => remember(item.path)}
            >
              <span>
                {item.path === "holding" ? frontText("Company-wide", language) : item.label}
              </span>
              <span aria-hidden="true">→</span>
            </Button>
          ))}
        </div>
      ) : (
        <InlineNotice tone="info">
          {language === "th"
            ? "ไม่พบสาขาหรือแผนกที่เปิดใช้ได้ ติดต่อผู้ดูแลบริษัท"
            : "No available branch or department. Contact your Company administrator."}
        </InlineNotice>
      )}
    </Card>
  );
}

