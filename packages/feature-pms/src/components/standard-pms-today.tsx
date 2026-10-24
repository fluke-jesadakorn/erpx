"use client";
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {PmsHotelSources} from "@erpx/feature-records/components/pms-hotel-sources";
import {CollectionRecordLink} from "@erpx/feature-records/components/record-collection";
import { Badge, Button, Card, Field, Select, Table, TableFrame, TextArea, TextInput } from "@erpx/ui/components/foundation";
import { PageHeader } from "@erpx/ui/components/primitives";
import { DraftFormDrawer } from "@erpx/ui/components/draft-form-drawer";
import type { ReactNode } from "react";

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { useSearchParams } from "@erpx/feature-shared/lib/destination-search";
import {
  ArrowUpRight,
  CheckCheck,
  Heart,
  Hotel,
  MessageCircle,
  RefreshCw,
  Send,
} from "lucide-react";
import { useFrontLanguage } from "@erpx/front/components/front-language";
import { SocialTextInput } from "@erpx/feature-collaboration/components/social-text-input";
import { SocialTextBody } from "@erpx/feature-collaboration/components/social-text-body";
import { PersonAvatar } from "@erpx/ui/components/person-avatar";
import { HandoverAcknowledgement } from "@erpx/feature-work_projection/components/today/handover-acknowledgement";
import { useMoneyFormatter } from "@erpx/feature-finance/components/money/currency-provider";
import { betaFetch } from "@erpx/feature-shared/lib/beta-client";
import { capturePmsHomeContext, sendPmsHomeCommand, type PmsHomeAttempt, type PmsHomeSource, type PmsHomeOutcome } from "@erpx/feature-pms/lib/home-command";
import { bangkokToday } from "@erpx/feature-shared/lib/handover";
import type { Mention } from "@erpx/server-collaboration/public/rules/social-text";
import type { RichNode } from "@erpx/feature-shared/lib/home-rich-content";
import type {HomePost} from '@erpx/server-collaboration/public/social-home';
import type { PmsHome, PmsHomeTask, PmsHomeView } from "@erpx/server-pms/public/pms/home";
import { scheduleWorkRefresh } from "@erpx/feature-work_projection/lib/work-refresh";
import { contextIsSwitching, hasUnsavedWork, registerUnsavedWork } from "@erpx/feature-work_projection/lib/context-switch";
import styles from "@erpx/feature-pms/components/standard-pms-today.module.css";

const queueLabels: Record<string, [string, string]> = {
  arrivals: ["รอเช็กอิน", "Arrivals"],
  departures: ["รอเช็กเอาต์", "Departures"],
  housekeeping: ["ห้องรอทำความสะอาด", "Rooms to prepare"],
  inspection: ["ห้องรอตรวจ", "Rooms to inspect"],
  transfers: ["เงินโอนรอตรวจ", "Transfers to verify"],
  corrections: ["แก้ค่าบริการ", "Charge corrections"],
  refunds: ["คืนเงิน", "Refunds"],
  variance: ["ส่วนต่างเงินสด", "Cash variances"],
  shift: ["กะแคชเชียร์", "Cashier shifts"],
  night: ["ปิดวันติดขัด", "Day-close blockers"],
};
const purposeLabels: Record<string, [string, string]> = {
  update: ["อัปเดต", "Update"],
  plan: ["แผนงาน", "Plan"],
  blocker: ["ขอความช่วยเหลือ", "Needs help"],
  handover: ["ส่งเวร", "Handover"],
  announcement: ["ประกาศ", "Announcement"],
};

export function StandardPmsToday({
  initial,
  api,
  frontHref,
  erpBase,
  branchId,
}: {
  initial: PmsHome;
  api: string;
  frontHref: string;
  erpBase: string;
  branchId: string | null;
}) {
  const language = useFrontLanguage(),
    th = language === "th";
  const t = (thai: string, english: string) => (th ? thai : english);
  const formatMoney = useMoneyFormatter();
  const search = useSearchParams();
  const [data, setData] = useState(initial),
    [view, setView] = useState<PmsHomeView>(() => {
      const saved = search.get("homeView");
      return saved && ["team", "mine", "handover", "company"].includes(saved)
        ? (saved as PmsHomeView)
        : initial.view;
    }),
    [property, setProperty] = useState(() =>
      initial.hotels.some((hotel) => hotel.id === search.get("homeProperty"))
        ? search.get("homeProperty")!
        : "",
    ),
    [label, setLabel] = useState(() =>
      Object.hasOwn(queueLabels, search.get("homeLabel") ?? "") ? search.get("homeLabel")! : "",
    );
  const [posts, setPosts] = useState(initial.posts),
    [pendingPosts, setPendingPosts] = useState<HomePost[] | null>(null),
    [more, setMore] = useState(initial.feedHasMore);
  const [loading, setLoading] = useState(false),
    [stale, setStale] = useState(false),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false);
  const [composer, setComposer] = useState(false),
    [body, setBody] = useState(""),
    [mentions, setMentions] = useState<Mention[]>([]),
    [doc, setDoc] = useState<RichNode | null>(null);
  const [purpose, setPurpose] = useState("update"),
    [selected, setSelected] = useState<PmsHomeTask[]>([]),
    [recipients, setRecipients] = useState<string[]>([]),
    [review, setReview] = useState(false);
  const [handoverBody, setHandoverBody] = useState("");
  const [handoverOpen,setHandoverOpen]=useState(false);
  const attempts = useRef(new Map<string, PmsHomeAttempt>()),
    generation = useRef(0);
  const [pendingAttempts, setPendingAttempts] = useState<PmsHomeAttempt[]>([]);
  const homeSource = useRef<PmsHomeSource | null>({ api, property, selectedBranch: branchId });
  const composerDraft = useRef({ body, mentions, doc, composer, handoverBody, selected, recipients, review });
  useLayoutEffect(() => {
    composerDraft.current = { body, mentions, doc, composer, handoverBody, selected, recipients, review };
  }, [body, mentions, doc, composer, handoverBody, selected, recipients, review]);
  const dirty = !!(body.trim() || handoverBody.trim() || selected.length || recipients.length || doc || busy || pendingAttempts.length);
  useEffect(() => {
    const unregister = registerUnsavedWork(() => dirty);
    const unload = (event: BeforeUnloadEvent) => { if (hasUnsavedWork() && !contextIsSwitching()) { event.preventDefault(); event.returnValue = ""; } };
    window.addEventListener("beforeunload", unload);
    return () => { unregister(); window.removeEventListener("beforeunload", unload); };
  }, [dirty]);
  const postsRef = useRef(posts);
  useEffect(() => {
    postsRef.current = posts;
  }, [posts]);
  const cancelRefresh = useCallback(() => {
    generation.current++;
  }, []);
  const [accessChanged, setAccessChanged] = useState(false);
  const query = new URLSearchParams({
    view,
    ...(property ? { property } : {}),
    ...(label ? { label } : {}),
  }).toString();
  const initialQuery = new URLSearchParams({ view: initial.view }).toString();
  const loadedQuery = useRef(initialQuery);
  const feedQuery = useRef(initialQuery);
  useEffect(() => {
    // Keep Today filters when visiting shared work and returning to Front.
    // `property` remains the existing operational hotel dashboard deep link.
    const url = new URL(window.location.href);
    for (const [key, value] of [
      ["homeProperty", property],
      ["homeView", view === "team" ? "" : view],
      ["homeLabel", label],
    ]) {
      if (value) url.searchParams.set(key, value);
      else url.searchParams.delete(key);
    }
    if (url.search !== window.location.search)
      window.history.replaceState(null, "", `${url.pathname}${url.search}${url.hash}`);
  }, [property, view, label]);
  const send = async (endpoint: string, input: unknown): Promise<PmsHomeOutcome> => {
    return sendPmsHomeCommand(attempts.current, () => homeSource.current, endpoint, input);
  };
  const refresh = useCallback(
    async (replace = false) => {
      const request = ++generation.current;
      setLoading(true);
      try {
        const next = await betaFetch<PmsHome>(
          `${api}?${query}&loaded=${encodeURIComponent(JSON.stringify(postsRef.current.map((post) => post.id)))}`,
        );
        if (request !== generation.current) return;
        setData(next);
        setStale(false);
        setError("");
        setAccessChanged(false);
        if (replace || feedQuery.current !== query) {
          setPosts(next.posts);
          setPendingPosts(null);
          setMore(next.feedHasMore);
          feedQuery.current = query;
        } else {
          const current = postsRef.current,
            ids = new Set(next.readablePostIds);
          if (next.posts.some((post) => !current.some((previous) => previous.id === post.id)))
            setPendingPosts(next.posts);
          setPosts(
            current
              .filter((post) => ids.has(post.id))
              .map((post) => next.posts.find((item) => item.id === post.id) ?? post),
          );
        }
      } catch (cause) {
        if (request === generation.current) {
          setStale(true);
          setError(cause instanceof Error ? cause.message : "Refresh failed");
          if ([401, 403, 404, 409].includes(Number((cause as { status?: number })?.status))) {
            setAccessChanged(true);
            setPosts([]);
            setPendingPosts(null);
            setSelected([]);
          }
        }
      } finally {
        if (request === generation.current) setLoading(false);
      }
    },
    [api, query],
  );
  useEffect(() => {
    if (loadedQuery.current === query) return;
    loadedQuery.current = query;
    void refresh(true);
    return cancelRefresh;
  }, [query, refresh, cancelRefresh]);
  useEffect(() => {
    const stop = scheduleWorkRefresh(() => void refresh());
    const timer = window.setInterval(() => {
      if (document.visibilityState === "visible") void refresh();
    }, 60_000);
    return () => {
      stop();
      window.clearInterval(timer);
      cancelRefresh();
    };
  }, [refresh, cancelRefresh]);
  const workHref = (item: PmsHomeTask) =>
    (["transfers", "corrections", "variance"].includes(item.label) ||
      (item.label === "refunds" && item.status === "requested")) &&
    item.canAct
      ? `${erpBase}${item.href}`
      : `${frontHref}${item.section}?${item.href.split("?")[1]}`;
  const total = data.counts.reduce((sum, item) => sum + item.count, 0),
    urgent = data.counts.reduce((sum, item) => sum + item.urgent, 0);
  const selectedHotel = data.hotels.find((hotel) => hotel.id === property);
  const selectedBranch = selectedHotel?.branchId ?? branchId;
  useLayoutEffect(() => {
    homeSource.current = { api, property, selectedBranch };
    return () => { homeSource.current = null; };
  }, [api, property, selectedBranch]);
  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Please try again.");
      if ([401, 403, 404, 409].includes(Number((cause as { status?: number })?.status)))
        await refresh();
    } finally {
      setPendingAttempts([...attempts.current.values()]);
      setBusy(false);
    }
  }
  async function publish() {
    const draft = JSON.stringify(composerDraft.current);
    await run(async () => {
      const outcome = await send(`${api}/publish`, {
        body,
        mentions,
        purpose,
        expectedVersion: 1,
        audience: selectedBranch ? { kind: "branch", id: selectedBranch } : { kind: "company" },
        ...(doc ? { richContent: { version: 1, blocks: [{ type: "text", doc }] } } : {}),
      });
      if (!outcome.acknowledge()) throw Error("Retry the submitted publication before continuing.");
      if (draft === JSON.stringify(composerDraft.current)) {
        setBody("");
        setMentions([]);
        setDoc(null);
        setComposer(false);
      }
      await refresh(true);
    });
  }
  async function publishHandover() {
    const draft = JSON.stringify(composerDraft.current);
    await run(async () => {
      const outcome = await send(`${api}/handover`, {
        date: bangkokToday(),
        body: handoverBody,
        recipientIds: recipients,
        sources: selected.map((item) => ({
          kind: "pms_record",
          id: item.id,
          sourceVersion: item.sourceVersion,
        })),
      });
      if (!outcome.acknowledge()) throw Error("Retry the submitted handover before continuing.");
      if (draft === JSON.stringify(composerDraft.current)) {
        setSelected([]);
        setRecipients([]);
        setReview(false);
        setHandoverBody("");
      }
      await refresh(true);
    });
  }
  async function loadTasks() {
    await run(async () => {
      const next = await betaFetch<PmsHome>(
        `${api}?${query}&taskCursor=${encodeURIComponent(data.tasks.nextCursor!)}`,
      );
      setData((current) => ({
        ...next,
        tasks: { ...next.tasks, items: [...current.tasks.items, ...next.tasks.items] },
      }));
    });
  }
  async function loadPosts() {
    await run(async () => {
      const last = posts.at(-1);
      if (!last) return;
      const next = await betaFetch<PmsHome>(
        `${api}?${query}&before=${last.created_at}&beforeId=${encodeURIComponent(last.id)}`,
      );
      setPosts((current) =>
        [
          ...current,
          ...next.posts.filter((post) => !current.some((item) => item.id === post.id)),
        ].slice(0, 100),
      );
      setMore(next.feedHasMore);
    });
  }
  function chooseView(next: PmsHomeView) {
    setView(next);
    setLabel("");
    setReview(false);
  }
  const recovery=(error || pendingAttempts.length > 0) ? (
        <div className={styles.error} role="alert">
          {error}
          <Button appearance="standard" type="submit" variant="plain" onClick={() => void refresh(true)}>
            {t("ลองอีกครั้ง", "Try again")}
          </Button>
          {pendingAttempts.map(attempt => <Button appearance="standard" key={attempt.key} type="button" disabled={busy} variant="plain" onClick={() => void run(async () => {
            const assertCurrent = capturePmsHomeContext(() => homeSource.current);
            const outcome = await send(attempt.endpoint, attempt.input);
            if (/\/posts\/[^/]+$/.test(attempt.endpoint)) {
              const updated = await betaFetch<HomePost>(attempt.endpoint);
              assertCurrent();
              if (!updated || updated.id !== outcome.data.id || !Number.isSafeInteger(updated.version) || updated.version <= 0 || outcome.data.version !== undefined && updated.version < outcome.data.version) throw Error("The post acknowledgement does not match the submitted source.");
            }
            await refresh(true);
            assertCurrent();
            if (!outcome.acknowledge()) throw Error("Retry the submitted action before continuing.");
          })}>{t("ลองคำขอเดิมอีกครั้ง (เก็บข้อความปัจจุบัน)", "Retry submitted action (keep current draft)")}</Button>)}
        </div>
  ) : null;
  if (accessChanged)
    return (
      <div className={styles.error} role="alert">
        {t(
          "สิทธิ์หรือบริบทงานเปลี่ยน กรุณาโหลดหน้าใหม่",
          "Your access or work context changed. Reload this page.",
        )}
        <Button appearance="standard" type="submit" variant="plain" onClick={() => window.location.reload()}>
          {t("โหลดหน้าใหม่", "Reload page")}
        </Button>
      </div>
    );
  return (
    <div className={styles.today} data-work-dirty={dirty ? "true" : undefined}><PmsHotelSources api={api} frontHref={frontHref}/>
      <PageHeader appearance="standard" eyebrow={t("งานโรงแรม · วันนี้", "HOTEL WORK · TODAY")} title={t("วันนี้ งานเดินหน้าไปด้วยกัน", "A good day starts together")} description={t("เห็นงานที่สำคัญ ส่งต่อให้ทีม และติดตามจนเรียบร้อย", "See what matters, hand it to the team, and follow it through.")} action={
        <Button appearance="standard"
          type="submit"
          variant="primary"
          className={styles.primary}
          onClick={() => {
            chooseView("handover");
            document.getElementById("pms-handover")?.scrollIntoView({ block: "center" });
          }}
        >
          <Send size={16} />
          {t("ส่งเวร", "Hand over")}
        </Button>
      } />
      <div className={styles.toolbar}>
        <Field appearance="standard">
          {t("โรงแรม", "Hotel")}
          <Select appearance="standard"
            value={property}
            onChange={(event) => {
              setProperty(event.target.value);
              setSelected([]);
              setReview(false);
            }}
          >
            <option value="">{t("ทุกโรงแรมที่เปิดได้", "All accessible hotels")}</option>
            {initial.hotels.map((hotel) => (
              <option key={hotel.id} value={hotel.id}>
                {hotel.name}
              </option>
            ))}
          </Select>
        </Field>
        <span role="status">
          {loading
            ? t("กำลังอัปเดต…", "Updating…")
            : stale
              ? t("ข้อมูลเก่า · ลองโหลดใหม่", "Older data · Try refreshing")
              : t("อัปเดตล่าสุด ", "Updated ") +
                new Intl.DateTimeFormat(th ? "th-TH-u-ca-gregory" : "en-GB", {
                  hour: "2-digit",
                  minute: "2-digit",
                  timeZone: "Asia/Bangkok",
                }).format(data.updatedAt)}
        </span>
        <Button appearance="standard"
          type="submit"
          variant="plain"
          aria-label={t("โหลดข้อมูลใหม่", "Refresh today")}
          disabled={loading}
          onClick={() => void refresh(true)}
        >
          <RefreshCw size={16} />
        </Button>
      </div>
      {recovery}
      <div
        className={styles.priorities}
        aria-label={t("งานที่ต้องทำก่อน", "Today's priorities")}
        aria-busy={loading}
      >
        <Button appearance="standard"
          type="submit"
          variant="plain"
          className={styles.priority}
          onClick={() => {
            chooseView("mine");
            document.getElementById("pms-work")?.scrollIntoView({ block: "start" });
          }}
        >
          <span>{t("งานที่ต้องทำ", "Work to follow up")}</span>
          <strong>{total}</strong>
          <small>
            {urgent
              ? t(`${urgent} งานเร่งด่วน`, `${urgent} urgent`)
              : t("เดินหน้าทีละงาน", "One step at a time")}
          </small>
        </Button>
        {data.counts
          .filter((item) => item.count > 0)
          .slice(0, 4)
          .map((item) => (
            <Button appearance="standard"
              type="submit"
              variant="plain"
              key={item.label}
              className={styles.priority}
              data-urgent={item.urgent > 0}
              onClick={() => {
                setView("team");
                setLabel(item.label);
                document.getElementById("pms-work")?.scrollIntoView({ block: "start" });
              }}
            >
              <span>{queueLabels[item.label]?.[th ? 0 : 1] ?? item.label}</span>
              <strong>{item.count}</strong>
              <small>{t("เปิดคิวงาน →", "Open queue →")}</small>
            </Button>
          ))}
      </div>
      <nav className={styles.views} aria-label={t("มุมมองวันนี้", "Today views")}>
        {(
          [
            ["team", "งานทีม", "Team work"],
            ["mine", "งานของฉัน", "My work"],
            ["handover", "ส่งเวร", "Handover"],
            ["company", "ภาพรวมบริษัท", "Company overview"],
          ] as const
        ).map(([value, thai, en]) => (
          <Button appearance="standard"
            type="submit"
            variant="plain"
            key={value}
            aria-pressed={view === value}
            onClick={() => chooseView(value)}
          >
            {t(thai, en)}
          </Button>
        ))}
      </nav>
      <div className={styles.columns}>
        <div className={styles.main}>
          {view === "company" && (
            <Card appearance="standard" className={styles.panel}>
              <div className={styles.sectionHead}>
                <h2>{t("ภาพรวมโรงแรมของบริษัท", "Hotels across the company")}</h2>
                <Hotel size={18} />
              </div>
              <TableFrame appearance="standard" label={t("ภาพรวมโรงแรมของบริษัท", "Hotels across the company")} className={styles.comparison}>
                <Table>
                  <thead>
                    <tr>
                      <th>{t("โรงแรม / วันที่ธุรกิจ", "Hotel / business date")}</th>
                      <th>Occupancy</th>
                      <th>ADR</th>
                      <th>RevPAR</th>
                      <th>{t("รายได้ค่าห้อง", "Room revenue")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.hotels.map((hotel) => (
                      <tr key={hotel.id}>
                        <td>
                          <CollectionRecordLink kind="pms_hotel_sources" id={hotel.id} args={{metric:'occupancy',businessDate:hotel.businessDate}}>{hotel.name}</CollectionRecordLink>
                          <small>{hotel.businessDate}</small>
                        </td>
                        <td>{hotel.occupancy === null ? "—" : <CollectionRecordLink kind="pms_hotel_sources" id={hotel.id} args={{metric:"occupancy",businessDate:hotel.businessDate}}>{`${(hotel.occupancy * 100).toFixed(1)}%`}</CollectionRecordLink>}</td>
                        {[hotel.adr, hotel.revpar, hotel.roomRevenue].map((amount, index) => (
                          <td key={index}>
                            {amount === null
                              ? "—"
                              : <CollectionRecordLink kind="pms_hotel_sources" id={hotel.id} args={{metric:['adr','revpar','room_revenue'][index],businessDate:hotel.businessDate}}>{formatMoney(Math.round(amount),hotel.currency,hotel.businessDate,2)}</CollectionRecordLink>}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
            </Card>
          )}
          <Card appearance="standard" className={styles.panel} id="pms-work" aria-busy={loading}>
            <div className={styles.sectionHead}>
              <h2>
                {label
                  ? queueLabels[label]?.[th ? 0 : 1]
                  : view === "mine"
                    ? t("งานของฉัน", "My work")
                    : t("เริ่มจากงานสำคัญ", "Start with what matters")}
              </h2>
              {label && (
                <Button appearance="standard" type="submit" variant="plain" onClick={() => setLabel("")}>
                  {t("ดูทั้งหมด", "Show all")}
                </Button>
              )}
            </div>
            {data.tasks.items.length ? (
              <div className={styles.tasks}>
                {data.tasks.items.map((item) => (
                  <article className={styles.task} key={item.id} data-urgent={item.urgent}>
                    {view === "handover" && (
                      <TextInput appearance="standard"
                        aria-label={t("เลือกส่งเวร ", "Hand over ") + item.title}
                        type="checkbox"
                        checked={selected.some((source) => source.id === item.id)}
                        disabled={
                          !selected.some((source) => source.id === item.id) && selected.length >= 5
                        }
                        onChange={() => {
                          setSelected((current) =>
                            current.some((source) => source.id === item.id)
                              ? current.filter((source) => source.id !== item.id)
                              : [...current, item],
                          );
                          setReview(false);
                        }}
                      />
                    )}
                    <div>
                      <WorkspaceLink href={workHref(item)}>
                        <strong>
                          {item.propertyName}
                          {item.title.split(" · ").length > 2
                            ? " · " + item.title.split(" · ")[1]
                            : ""}
                        </strong>
                        <ArrowUpRight size={15} />
                      </WorkspaceLink>
                      <span>
                        {queueLabels[item.label]?.[th ? 0 : 1]} · {item.status}
                        {!item.canAct
                          ? " · " + t("รอตรวจสอบอิสระ", "Awaiting independent review")
                          : ""}
                      </span>
                      <small>
                        {t("วันที่ธุรกิจ ", "Business date ")}
                        {item.businessDate}
                        {item.urgent ? " · " + t("เร่งด่วน", "Urgent") : ""}
                      </small>
                    </div>
                    <WorkspaceLink className={styles.openWork} href={workHref(item)}>
                      {t("เปิดงาน", "Open work")}
                    </WorkspaceLink>
                  </article>
                ))}
              </div>
            ) : (
              <div className={styles.empty}>
                <CheckCheck size={26} />
                <strong>
                  {loading
                    ? t("กำลังโหลดงาน", "Loading work")
                    : t("ไม่มีงานค้างในมุมมองนี้", "No pending work in this view")}
                </strong>
                <p>
                  {t("งานใหม่ที่เกี่ยวข้องจะแสดงที่นี่", "Relevant new work will appear here.")}
                </p>
              </div>
            )}
            {data.tasks.nextCursor && (
              <Button appearance="standard"
                type="submit"
                variant="plain"
                disabled={busy || loading}
                onClick={() => void loadTasks()}
              >
                {t("ดูงานเพิ่มเติม", "More work")}
              </Button>
            )}
          </Card>
          {view === "handover" && <>
            <Card appearance="standard" id="pms-handover" className={styles.handoverStart}>
              <h2>{t("ส่งต่ออย่างมั่นใจ", "Leave the next shift ready")}</h2>
              <p>{t("เลือกงานด้านบน แล้วเปิดข้อความส่งเวรเพื่อระบุผู้รับและตรวจทาน", "Select work above, then open the handover message to choose recipients and review it.")}</p>
              <Button appearance="standard" type="button" disabled={busy || pendingAttempts.length > 0} onClick={() => setHandoverOpen(true)}>{t("เตรียมส่งเวร", "Prepare handover")}</Button>
            </Card>
            <StandardPmsHomeEditor title={t("ส่งเวร", "Handover")} language={language} open={handoverOpen} busy={busy} pending={pendingAttempts.length > 0} dirty={!!(handoverBody.trim() || selected.length || recipients.length)} onClose={() => setHandoverOpen(false)} recovery={recovery}>
            <Card appearance="standard" className={styles.panel}>
              <div className={styles.sectionHead}>
                <h2>{t("ส่งต่ออย่างมั่นใจ", "Leave the next shift ready")}</h2>
                <Send size={18} />
              </div>
              <p className={styles.muted}>
                {t(
                  "เลือกงานด้านบน ระบุผู้รับ และตรวจข้อความก่อนส่ง",
                  "Choose work above, select recipients, and review your message.",
                )}
              </p>
              <Field appearance="standard">
                {t("ข้อความส่งเวร", "Handover message")}
                <TextArea appearance="standard"
                  value={handoverBody}
                  maxLength={3000}
                  onChange={(event) => {
                    setHandoverBody(event.target.value);
                    setReview(false);
                  }}
                  placeholder={t(
                    "ทำอะไรแล้ว · มีอะไรค้าง · เวรถัดไปต้องทำอะไร",
                    "What is done, what is pending, and what the next shift should do",
                  )}
                />
              </Field>
              <fieldset disabled={busy}>
                <legend>{t("ผู้รับ", "Recipients")}</legend>
                <div className={styles.people}>
                  {data.people.map((person) => (
                    <Field appearance="standard" orientation="inline" key={person.id}>
                      <TextInput appearance="standard"
                        type="checkbox"
                        checked={recipients.includes(person.id)}
                        disabled={!recipients.includes(person.id) && recipients.length >= 20}
                        onChange={() => {
                          setRecipients((current) =>
                            current.includes(person.id)
                              ? current.filter((id) => id !== person.id)
                              : [...current, person.id],
                          );
                          setReview(false);
                        }}
                      />
                      {person.name}
                    </Field>
                  ))}
                </div>
              </fieldset>
              <p className={styles.muted}>
                {selected.length} / 5{" "}
                {t(
                  "งานที่เลือก · ผู้รับต้องเปิดทุกงานได้",
                  "selected items · Recipients must be able to open every item",
                )}
              </p>
              {review ? (
                <div className={styles.review}>
                  <strong>{t("ตรวจส่งเวร", "Review handover")}</strong>
                  <p>
                    {recipients
                      .map((id) => data.people.find((person) => person.id === id)?.name)
                      .join(", ")}
                  </p>
                  <ul>
                    {selected.map((item) => (
                      <li key={item.id}>{item.title}</li>
                    ))}
                  </ul>
                  <p>{handoverBody}</p>
                  <Button appearance="standard" type="submit" variant="plain" onClick={() => setReview(false)}>
                    {t("แก้ไข", "Edit")}
                  </Button>
                  <Button appearance="standard"
                    type="submit"
                    variant="primary"
                    className={styles.primary}
                    disabled={busy}
                    onClick={() => void publishHandover()}
                  >
                    {t("ยืนยันส่งเวร", "Publish handover")}
                  </Button>
                </div>
              ) : (
                <Button appearance="standard"
                  type="submit"
                  variant="primary"
                  className={styles.primary}
                  disabled={busy || !selected.length || !recipients.length || !handoverBody.trim()}
                  onClick={() => setReview(true)}
                >
                  {t("ตรวจข้อความก่อนส่ง", "Review before sending")}
                </Button>
              )}
            </Card>
            </StandardPmsHomeEditor>
          </>}
          <section
            className={styles.feed}
            aria-label={t("ฟีดทีมร่วมกับ ERPX", "Shared ERPX team feed")}
          >
            <div className={styles.sectionHead}>
              <div>
                <p className={styles.eyebrow}>
                  {t("เชื่อมทีมเข้าด้วยกัน", "KEEP THE TEAM CONNECTED")}
                </p>
                <h2>{t("เรื่องราวของวันนี้", "Around the team")}</h2>
              </div>
              <Button appearance="standard" type="submit" variant="plain" disabled={busy || pendingAttempts.length > 0} onClick={() => setComposer(!composer)}>
                <MessageCircle size={16} />
                {t("เขียนโพสต์", "Write a post")}
              </Button>
            </div>
            <StandardPmsHomeEditor title={t("เขียนโพสต์", "Write a post")} language={language} open={composer} busy={busy} pending={pendingAttempts.length > 0} dirty={!!(body.trim() || doc || mentions.length)} onClose={() => setComposer(false)} recovery={recovery}>
              <div className={styles.panel}>
                <Field appearance="standard">
                  {t("ประเภทโพสต์", "Post purpose")}
                  <Select appearance="standard" value={purpose} onChange={(event) => setPurpose(event.target.value)}>
                    {["update", "plan", "blocker", "announcement"].map((value) => (
                      <option key={value} value={value}>
                        {purposeLabels[value][th ? 0 : 1]}
                      </option>
                    ))}
                  </Select>
                </Field>
                <SocialTextInput
                  api={api}
                  value={body}
                  mentions={mentions}
                  richDoc={doc}
                  maxLength={3000}
                  disabled={busy}
                  label={t("ข้อความถึงทีม", "Message to the team")}
                  placeholder={t(
                    "แชร์สิ่งที่ทีมควรรู้ หรือขอบคุณเพื่อนร่วมงาน…",
                    "Share something the team should know, or thank a colleague…",
                  )}
                  onChange={(value, next) => {
                    setBody(value);
                    setMentions(next);
                    setDoc(null);
                  }}
                  onRichChange={(value, next, rich) => {
                    setBody(value);
                    setMentions(next);
                    setDoc(rich as RichNode);
                  }}
                />
                <p className={styles.muted}>
                  {t("ผู้ชม: ", "Audience: ")}
                  {selectedBranch
                    ? (selectedHotel?.name ?? t("สาขานี้", "This branch"))
                    : t("บริษัทนี้", "This company")}
                </p>
                <Button appearance="standard"
                  type="submit"
                  variant="primary"
                  className={styles.primary}
                  disabled={busy || !body.trim()}
                  onClick={() => void publish()}
                >
                  {busy ? t("กำลังส่ง…", "Publishing…") : t("เผยแพร่", "Publish")}
                </Button>
              </div>
            </StandardPmsHomeEditor>
            {pendingPosts && (
              <Button appearance="standard"
                type="submit"
                variant="plain"
                className={styles.newPosts}
                onClick={() => {
                  setPosts(pendingPosts);
                  setPendingPosts(null);
                  setMore(pendingPosts.length === 8);
                }}
              >
                {t("มีเรื่องใหม่ · กดเพื่อแสดง", "New posts · Show them")}
              </Button>
            )}
            {posts.length ? (
              posts.map((post) => (
                <PmsFeedPost
                  key={post.id}
                  post={post}
                  api={api}
                  erpBase={erpBase}
                  frontHref={frontHref}
                  userId={data.userId}
                  language={language}
                  onChanged={() => refresh()}
                  send={send}
                  retainedAttempt={(endpoint) => [...attempts.current.values()].find(attempt => attempt.endpoint === endpoint)}
                  onPending={() => setPendingAttempts([...attempts.current.values()])}
                />
              ))
            ) : (
              <Card appearance="standard" className={`${styles.panel} ${styles.empty}`}>
                <MessageCircle size={26} />
                <strong>
                  {t("เริ่มเรื่องราวของทีมวันนี้", "Start today's team conversation")}
                </strong>
                <p>
                  {t(
                    "ประกาศ อัปเดต และส่งเวรใช้โพสต์เดียวกับ ERPX",
                    "Announcements, updates, and handovers share the same ERPX posts.",
                  )}
                </p>
              </Card>
            )}
            {more && posts.length < 100 && (
              <Button appearance="standard"
                type="submit"
                variant="plain"
                disabled={busy}
                onClick={() => void loadPosts()}
              >
                {t("ดูโพสต์ก่อนหน้า", "Earlier posts")}
              </Button>
            )}
          </section>
        </div>
        <aside className={styles.aside} aria-label={t("ภาพรวมโรงแรม", "Hotel overview")}>
          <div className={styles.sectionHead}>
            <h2>{t("โรงแรมวันนี้", "Hotels today")}</h2>
            <Hotel size={18} />
          </div>
          {new Set(data.hotels.map((hotel) => hotel.businessDate)).size > 1 && (
            <p className={styles.notice}>
              {t(
                "โรงแรมมีวันที่ธุรกิจต่างกัน ดูวันที่กำกับแต่ละยอด",
                "Hotels have different business dates. Read each date with its totals.",
              )}
            </p>
          )}
          {data.hotels.map((hotel) => (
            <Card appearance="standard" as="article" className={styles.hotel} key={hotel.id}>
              <div className={styles.sectionHead}>
                <h3>{hotel.name}</h3>
                <Badge>{hotel.state}</Badge>
              </div>
              <small>
                {t("วันที่ธุรกิจ ", "Business date ")}
                {hotel.businessDate}
              </small>
              <div className={styles.readiness}>
                <strong>
                  {hotel.inspected} / {hotel.roomCount}
                </strong>
                <span>{t("ห้องตรวจแล้ว", "rooms inspected")}</span>
              </div>
              <progress
                aria-label={t("ห้องพร้อมใช้ ", "Room readiness ") + hotel.name}
                max={hotel.roomCount || 1}
                value={hotel.inspected}
              />
              <dl>
                <div>
                  <dt>{t("เข้าพัก", "Occupancy")}</dt>
                  <dd>
                    {hotel.occupancy === null ? "—" : Math.round(hotel.occupancy * 100) + "%"}{" "}
                    <small>
                      {hotel.sold}/{hotel.capacity}
                    </small>
                  </dd>
                </div>
                <div>
                  <dt>{t("รอเข้าพัก / ออก", "Arrivals / departures")}</dt>
                  <dd>
                    {hotel.arrivals} / {hotel.departures}
                  </dd>
                </div>
                {hotel.roomRevenue !== null && (
                  <>
                    <div>
                      <dt>{t("รายได้ค่าห้อง", "Room revenue")}</dt>
                      <dd>
                        {formatMoney(hotel.roomRevenue, hotel.currency, hotel.businessDate, 2)}
                      </dd>
                    </div>
                    <div>
                      <dt>ADR / RevPAR</dt>
                      <dd>
                        {hotel.adr === null
                          ? "—"
                          : formatMoney(
                              Math.round(hotel.adr),
                              hotel.currency,
                              hotel.businessDate,
                              2,
                            )}{" "}
                        /{" "}
                        {hotel.revpar === null
                          ? "—"
                          : formatMoney(
                              Math.round(hotel.revpar),
                              hotel.currency,
                              hotel.businessDate,
                              2,
                            )}
                      </dd>
                    </div>
                    <div>
                      <dt>{t("ยอดค้าง", "Receivables")}</dt>
                      <dd>
                        {formatMoney(hotel.receivable ?? 0, hotel.currency, hotel.businessDate, 2)}
                      </dd>
                    </div>
                  </>
                )}
              </dl>
              <WorkspaceLink
                className={styles.hotelLink}
                href={`${frontHref}dashboard?property=${encodeURIComponent(hotel.id)}`}
              >
                {t("เปิดโรงแรม", "Open hotel")}
                <ArrowUpRight size={15} />
              </WorkspaceLink>
            </Card>
          ))}
          {!data.hotels.length && (
            <p>{t("ยังไม่มีโรงแรมที่เปิดได้", "No accessible hotels yet.")}</p>
          )}
          <p className={styles.muted}>
            {t(
              "บัญชี PMS ใช้ THB · ยอดแปลงเป็นข้อมูลประกอบ",
              "PMS accounting uses THB. Display conversions are for reference.",
            )}
          </p>
          {data.canFinance && (
            <WorkspaceLink href={canonicalUiHref(`${erpBase}/finance/hotels`)}>
              {t("ดูบัญชีโรงแรมใน Finance →", "Hotel accounting in Finance →")}
            </WorkspaceLink>
          )}
        </aside>
      </div>
    </div>
  );
}

function PmsFeedPost({
  post,
  api,
  erpBase,
  frontHref,
  userId,
  language,
  onChanged,
  send,
  retainedAttempt,
  onPending,
}: {
  post: HomePost;
  api: string;
  erpBase: string;
  frontHref: string;
  userId: string;
  language: "th" | "en";
  onChanged: () => Promise<void>;
  send: (endpoint: string, input: unknown) => Promise<PmsHomeOutcome>;
  retainedAttempt: (endpoint: string) => PmsHomeAttempt | undefined;
  onPending: () => void;
}) {
  const th = language === "th",
    t = (thai: string, en: string) => (th ? thai : en);
  const [current, setCurrent] = useState(post),
    [comments, setComments] = useState(false),
    [body, setBody] = useState(""),
    [mentions, setMentions] = useState<Mention[]>([]),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const [retryPending, setRetryPending] = useState(false);
  const [previousPost, setPreviousPost] = useState(post);
  if (previousPost !== post) {
    setPreviousPost(post);
    setCurrent(post);
  }
  const rich = current.richContent?.blocks.find((block) => block.type === "text");
  const postSource = useRef<PmsHomeSource | null>({ api, property: post.id, selectedBranch: post.branch_id });
  useLayoutEffect(() => {
    postSource.current = { api, property: post.id, selectedBranch: post.branch_id };
    return () => { postSource.current = null; };
  }, [api, post.id, post.branch_id]);
  const commentDraft = useRef({ body, mentions });
  useLayoutEffect(() => { commentDraft.current = { body, mentions }; }, [body, mentions]);
  async function act(input: unknown) {
    if (busy) return false;
    setBusy(true);
    setError("");
    try {
      const assertCurrent = capturePmsHomeContext(() => postSource.current);
      const outcome = await send(`${api}/posts/${post.id}`, input);
      const updated = await betaFetch<HomePost>(`${api}/posts/${post.id}`);
      assertCurrent();
      if (!updated || updated.id !== post.id || updated.company_id !== post.company_id || updated.branch_id !== post.branch_id || updated.department_id !== post.department_id || !Number.isSafeInteger(updated.version) || updated.version <= 0 || outcome.data.version !== undefined && updated.version < outcome.data.version)
        throw Error("The post acknowledgement does not match the displayed source.");
      await onChanged();
      assertCurrent();
      if (!outcome.acknowledge()) throw Error("Retry the submitted action before continuing.");
      setCurrent(updated);
      return true;
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Please try again.");
      if ([401, 403, 404, 409].includes(Number((cause as { status?: number })?.status)))
        await onChanged();
      return false;
    } finally {
      setRetryPending(!!retainedAttempt(`${api}/posts/${post.id}`));
      onPending();
      setBusy(false);
    }
  }
  async function retrySubmitted() {
    const pending = retainedAttempt(`${api}/posts/${post.id}`);
    if (!pending) return;
    const input = pending.input;
    const success = await act(input);
    if (success && input && typeof input === "object" && "action" in input && input.action === "comment" && "body" in input && typeof input.body === "string" && "mentions" in input
      && commentDraft.current.body === input.body && JSON.stringify(commentDraft.current.mentions) === JSON.stringify(input.mentions)) {
      setBody("");
      setMentions([]);
    }
  }
  const sourceHref = (href: string) =>
    href.includes("queue=approval") || href.includes("queue=corrections")
      ? `${erpBase}${href}`
      : `${frontHref}${href.replace(/^\/pms\//, "")}`;
  return (
    <Card appearance="standard" as="article" className={styles.post} data-work-dirty={body.trim() || busy || retryPending ? "true" : undefined}>
      <header>
        <PersonAvatar name={current.author_name} />
        <div>
          <strong>{current.author_name}</strong>
          <p>
            {purposeLabels[current.purpose]?.[th ? 0 : 1]} · {current.audienceLabel}
          </p>
          <WorkspaceLink href={canonicalUiHref(`${erpBase}/posts/${current.id}`)}>
            <time dateTime={new Date(current.created_at).toISOString()}>
              {new Intl.DateTimeFormat(th ? "th-TH-u-ca-gregory" : "en-GB", {
                timeZone: "Asia/Bangkok",
                month: "short",
                day: "numeric",
                hour: "2-digit",
                minute: "2-digit",
              }).format(current.created_at)}
            </time>
          </WorkspaceLink>
        </div>
      </header>
      <SocialTextBody
        body={current.body}
        mentions={current.mentions}
        base={erpBase}
        doc={rich?.type === "text" ? rich.doc : null}
      />
      {!!current.files.length && (
        <WorkspaceLink href={canonicalUiHref(`${erpBase}/posts/${current.id}`)}>
          {t("เปิดไฟล์แนบใน ERPX", "Open attachments in ERPX")} · {current.files.length}
        </WorkspaceLink>
      )}
      <div className={styles.sourceLinks}>
        {current.linkedSources.map((source, index) =>
          source.available ? (
            <WorkspaceLink
              href={
                source.kind === "pms_record" ? sourceHref(source.href) : `${erpBase}${source.href}`
              }
              key={source.id}
            >
              {source.title}
              <ArrowUpRight size={14} />
            </WorkspaceLink>
          ) : (
            <span key={index}>{t("รายการต้นทางเปิดไม่ได้แล้ว", "Source no longer available")}</span>
          ),
        )}
      </div>
      {current.purpose === "handover" && current.audience_kind === "participants" && (
        <HandoverAcknowledgement
          api={api}
          postId={current.id}
          version={current.version}
          language={language}
          initial={current.handoverReceipt}
          contextToken={api + ":" + userId}
          purchase={current.purchase}
        />
      )}
      <footer>
        <Button appearance="standard"
          type="submit"
          variant="plain"
          disabled={busy}
          aria-pressed={!!current.appreciated}
          onClick={() => void act({ action: "appreciate", active: !current.appreciated })}
        >
          <Heart size={16} />
          {t("ขอบคุณ", "Thank you")} {current.appreciationCount || ""}
        </Button>
        <Button appearance="standard"
          type="submit"
          variant="plain"
          aria-expanded={comments}
          onClick={() => {
            setComments(!comments);
            if (!comments) {
              const assertCurrent = capturePmsHomeContext(() => postSource.current);
              void betaFetch<HomePost>(`${api}/posts/${post.id}`)
                .then(updated => { assertCurrent(); if (updated.id !== post.id) throw Error("The post does not match the displayed source."); setCurrent(updated); })
                .catch((cause) => setError(cause.message));
            }
          }}
        >
          <MessageCircle size={16} />
          {t("ความคิดเห็น", "Comments")} {current.commentCount || ""}
        </Button>
        <WorkspaceLink href={canonicalUiHref(`${erpBase}/posts/${current.id}`)}>
          {t("เปิดใน ERPX", "Open in ERPX")}
          <ArrowUpRight size={14} />
        </WorkspaceLink>
      </footer>
      {comments && (
        <section className={styles.comments} aria-label={t("ความคิดเห็น", "Comments")}>
          {current.comments.map((comment) => (
            <div key={comment.id}>
              <strong>{comment.author_name}</strong>
              <SocialTextBody body={comment.body} mentions={comment.mentions} base={erpBase} />
            </div>
          ))}
          {current.commentCount > current.comments.length && (
            <WorkspaceLink href={canonicalUiHref(`${erpBase}/posts/${current.id}`)}>
              {t("ดูความคิดเห็นทั้งหมด", "Read all comments")}
            </WorkspaceLink>
          )}
          <SocialTextInput
            api={api}
            value={body}
            mentions={mentions}
            maxLength={1000}
            disabled={busy}
            showHint={false}
            label={t("เขียนความคิดเห็น", "Write a comment")}
            placeholder={t("ตอบกลับทีม…", "Reply to the team…")}
            onChange={(value, next) => {
              setBody(value);
              setMentions(next);
            }}
          />
          <Button appearance="standard"
            type="submit"
            variant="plain"
            disabled={busy || !body.trim()}
            onClick={() =>
              void act({ action: "comment", body, mentions }).then((success) => {
                if (success && commentDraft.current.body === body && JSON.stringify(commentDraft.current.mentions) === JSON.stringify(mentions)) {
                  setBody("");
                  setMentions([]);
                }
              })
            }
          >
            {t("ส่งความคิดเห็น", "Send comment")}
          </Button>
        </section>
      )}
      {error && (
        <div role="alert" className={styles.error}>
          <p>{error}</p>
          {retryPending && <Button appearance="standard" type="button" variant="plain" disabled={busy} onClick={() => void retrySubmitted()}>
            {t("ลองคำขอเดิมอีกครั้ง (เก็บข้อความใหม่)", "Retry submitted action (keep newer draft)")}
          </Button>}
        </div>
      )}
      {current.kind === "team" && current.author_id === userId && (
        <span className={styles.muted}>{t("แก้ไขโพสต์ได้จาก ERPX", "Edit this post in ERPX")}</span>
      )}
    </Card>
  );
}

/** Long editors retain their controlled owner drafts and exact pending recovery.
 * Closing never submits or resets content; it is blocked while the native result
 * is unknown. The original publisher owns all saved/newer-draft decisions. */
export function StandardPmsHomeEditor({title,language,open,busy,pending,dirty,onClose,recovery,children}:{
 title:string;language:"th"|"en";open:boolean;busy:boolean;pending:boolean;dirty:boolean;onClose:()=>void;recovery:ReactNode;children:ReactNode;
}){
 const close=()=>{if(!busy&&!pending)onClose();},th=language==='th';
 return <DraftFormDrawer appearance="standard" title={title} open={open} dirty={dirty} dismissDisabled={busy||pending} retainFormContents={dirty||pending||busy} onClose={close} onDiscard={close}
  confirmationTitle={th?'ปิดข้อความที่ยังไม่เผยแพร่หรือไม่?':'Close unpublished text?'} confirmationDescription={th?'ข้อความยังไม่ได้เผยแพร่ ยังเก็บข้อความไว้ในหน้านี้':'This text has not been published. Its draft remains in this view.'} discardLabel={th?'ปิดโดยไม่เผยแพร่':'Close without publishing'} keepEditingLabel={th?'แก้ไขต่อ':'Keep editing'}>
  {recovery}{children}
 </DraftFormDrawer>;
}
