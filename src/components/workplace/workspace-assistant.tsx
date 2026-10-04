"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useCallback, useEffect, useRef, useState } from "react";
import { usePathname, useSearchParams } from "next/navigation";
import {
  BookOpen,
  MessageSquare,
  Send,
  ShieldCheck,
  Sparkles,
  X,
  LoaderCircle,
  CirclePlus,
} from "lucide-react";
import Link from "next/link";
import { assistantPage, assistantSupport, type AssistantView } from "@/lib/ai/workspace-assistant";
import {
  intakeFields,
  intakeMissing,
  saveGuidedIntakeHandoff,
  type GuidedIntakeAction,
  type GuidedIntakeResult,
} from "@/lib/ai/guided-intake";
import { workFetch } from "@/lib/work-fetch";
import { useHydrated } from "@/components/ui/use-hydrated";
import styles from "./workspace-assistant.module.css";

export function WorkspaceAssistant({
  base,
  userId,
  companyId,
  assignmentId,
  crossContext,
}: {
  base: string;
  userId: string;
  companyId: string;
  assignmentId: string;
  crossContext: boolean;
}) {
  const pathname = usePathname(),
    search = useSearchParams();
  const ready = useHydrated();
  const [open, setOpen] = useState(false),
    [mode, setMode] = useState<"page" | "workspace">("page"),
    [tab, setTab] = useState<"intake" | "chat" | "guide">("chat");
  const [startPrompt, setStartPrompt] = useState("");
  const [startSequence, setStartSequence] = useState(0);
  const [contexts, setContexts] = useState<string[]>([]);
  const launcher = useRef<HTMLButtonElement>(null),
    closeButton = useRef<HTMLButtonElement>(null);
  let path: string | null = null;
  try {
    path = assistantPage(`${pathname.slice(base.length)}${search.size ? `?${search}` : ""}`).path;
  } catch {}
  const close = useCallback(() => {
    setOpen(false);
    setStartPrompt("");
    requestAnimationFrame(() => launcher.current?.focus());
  }, []);
  useEffect(() => {
    const openFromStart = (event: Event) => {
      const detail = (event as CustomEvent<{ question?: unknown; mode?: unknown }>).detail;
      const question = detail?.question;
      setStartPrompt(typeof question === "string" ? question.slice(0, 4000) : "");
      setStartSequence((value) => value + 1);
      setTab(detail?.mode === "intake" ? "intake" : "chat");
      setOpen(true);
    };
    window.addEventListener("erpx:open-assistant", openFromStart);
    return () => window.removeEventListener("erpx:open-assistant", openFromStart);
  }, []);
  useEffect(() => {
    if (!open) return;
    closeButton.current?.focus();
    const handle = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    };
    window.addEventListener("keydown", handle);
    return () => window.removeEventListener("keydown", handle);
  }, [open, close]);
  if (!path) return null;
  const guide = assistantPage(path).guide;
  return (
    <div data-app-chrome>
      {!open && (
        <Button appearance="standard"
          type="submit"
          variant="ghost"
          ref={launcher}
          disabled={!ready}
          className={styles.launcher}
          onClick={() => setOpen(true)}
          aria-label="Open work assistant"
        >
          <Sparkles size={19} />
          <span>Ask ERPX</span>
        </Button>
      )}
      {open && (
        <aside className={styles.panel} aria-label="ERPX work assistant">
          <header className={styles.header}>
            <div className={styles.brand}>
              <Sparkles />
              <div>
                <strong>Ask ERPX</strong>
                <small>
                  {assistantSupport(path) === "prepare"
                    ? "Guides · Draft preparation and checks"
                    : assistantSupport(path) === "read"
                      ? "Guides · Authorized records and summaries"
                      : "Workflow guides"}
                </small>
              </div>
            </div>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              ref={closeButton}
              className={styles.icon}
              onClick={close}
              aria-label="Close work assistant"
            >
              <X size={20} />
            </Button>
          </header>
          {tab === "intake" ? (
            <div className={styles.context}>
              <ShieldCheck size={14} /> Your work context controls which forms you can open. You
              review every draft before saving or submitting.
            </div>
          ) : (
            <div className={styles.context}>
              <Field appearance="standard">
                <ShieldCheck size={14} />
                {crossContext ? (
                  <Select appearance="standard"
                    aria-label="Assistant context scope"
                    value={mode}
                    onChange={(e) => {
                      setMode(e.target.value as typeof mode);
                      setContexts([]);
                    }}
                  >
                    <option value="page">This page · {guide.title}</option>
                    <option value="workspace">Follow my work · permitted pages</option>
                  </Select>
                ) : (
                  <strong>This page · {guide.title}</strong>
                )}
              </Field>
              <p>
                {mode === "page"
                  ? "Separate conversation for this page and record."
                  : "Current page plus the contexts you keep below. Your selected role still applies."}
              </p>
              {mode === "workspace" && (
                <div className={styles.chips}>
                  {contexts.map((p) => (
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
                      key={p}
                      onClick={() => setContexts((c) => c.filter((v) => v !== p))}
                      aria-label={`Remove context ${p}`}
                    >
                      {assistantPage(p).guide.title} ×
                    </Button>
                  ))}
                  {!contexts.includes(path) && contexts.length < 5 && (
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
                      onClick={() => setContexts((c) => [...c, path!])}
                    >
                      + Keep {guide.title} as context
                    </Button>
                  )}
                </div>
              )}
            </div>
          )}
          <div className={styles.tabs}>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              aria-pressed={tab === "guide"}
              onClick={() => setTab("guide")}
            >
              <BookOpen size={15} />
              Explain
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              aria-pressed={tab === "intake"}
              onClick={() => setTab("intake")}
            >
              <CirclePlus size={15} />
              Prepare
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              onClick={() => {
                setTab("chat");
                setStartPrompt(
                  "Check the supplied information for missing details, contradictions and items the responsible person should verify. State what you cannot verify.",
                );
              }}
            >
              <ShieldCheck size={15} />
              Check
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              onClick={() => {
                setTab("chat");
                setStartPrompt(
                  "Summarize this work, current status, owner, next step and blockers using the available sources. State any information that is missing.",
                );
              }}
            >
              <MessageSquare size={15} />
              Summarize
            </Button>
          </div>
          {tab === "guide" ? (
            <div className={styles.body}>
              <div className={styles.empty}>
                <h3>Work confidently in {guide.title}</h3>
                <p>{guide.purpose}</p>
              </div>
              <ol className={styles.steps}>
                {guide.steps.map((s) => (
                  <li key={s}>{s}</li>
                ))}
              </ol>
              <p className={styles.notice}>
                Workflow guidance is available even when AI is offline. Company rules come from your
                published handbook.
              </p>
              <Link href={`${base}/handbook`}>Open company handbook →</Link>
            </div>
          ) : tab === "intake" ? (
            <GuidedIntakePanel
              key={startSequence}
              base={base}
              userId={userId}
              companyId={companyId}
              assignmentId={assignmentId}
              path={path}
              initialQuestion={startPrompt}
              onContinue={() => setOpen(false)}
            />
          ) : (
            <AssistantConversation
              key={`${mode === "page" ? path : "workspace"}:${startPrompt}`}
              base={base}
              path={path}
              mode={mode}
              contextPaths={contexts}
              initialQuestion={startPrompt}
            />
          )}
        </aside>
      )}
    </div>
  );
}

function GuidedIntakePanel({
  base,
  userId,
  companyId,
  assignmentId,
  path,
  initialQuestion,
  onContinue,
}: {
  base: string;
  userId: string;
  companyId: string;
  assignmentId: string;
  path: string;
  initialQuestion: string;
  onContinue: () => void;
}) {
  const [question, setQuestion] = useState(initialQuestion),
    [result, setResult] = useState<GuidedIntakeResult | null>(null),
    [draft, setDraft] = useState<Record<string, string>>({});
  const [actions, setActions] = useState<GuidedIntakeAction[]>([]),
    [unavailable, setUnavailable] = useState<string | null>(null);
  const [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const autoStarted = useRef(false),
    attempt = useRef<{ message: string; key: string } | null>(null);
  useEffect(() => {
    const abort = new AbortController();
    void (async () => {
      try {
        const response = await workFetch(`/api/v1${base}/assistant/intake`, {
          cache: "no-store",
          signal: abort.signal,
        });
        const body = await response.json();
        if (!response.ok) throw Error(body.error?.message ?? "Unable to load available forms.");
        if (!abort.signal.aborted) {
          setActions(body.data.actions as GuidedIntakeAction[]);
          setUnavailable(body.data.unavailable as string | null);
        }
      } catch (error) {
        if (!abort.signal.aborted)
          setError(error instanceof Error ? error.message : "Unable to load available forms.");
      } finally {
        if (!abort.signal.aborted) setLoading(false);
      }
    })();
    return () => abort.abort();
  }, [base]);
  const prepare = useCallback(
    async (message: string) => {
      if (!message.trim()) return;
      setBusy(true);
      setError("");
      setResult(null);
      if (attempt.current?.message !== message)
        attempt.current = { message, key: crypto.randomUUID() };
      try {
        const response = await workFetch(`/api/v1${base}/assistant/intake`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "Idempotency-Key": attempt.current.key },
          body: JSON.stringify({ message, contextPath: path }),
        });
        const body = await response.json();
        if (!response.ok)
          throw Error(
            body.error?.message ?? "AI could not prepare this request. Choose a form below.",
          );
        const next = body.data as GuidedIntakeResult;
        setResult(next);
        setDraft(next.draft);
        setActions(next.actions);
        attempt.current = null;
      } catch (cause) {
        setError(
          cause instanceof Error ? cause.message : "AI is unavailable. Choose a form below.",
        );
      } finally {
        setBusy(false);
      }
    },
    [base, path],
  );
  useEffect(() => {
    if (autoStarted.current || !initialQuestion.trim() || loading || unavailable) return;
    autoStarted.current = true;
    void prepare(initialQuestion);
  }, [initialQuestion, loading, unavailable, prepare]);
  const fields = result?.intent && result.intent !== "unknown" ? intakeFields[result.intent] : [];
  return (
    <>
      <div className={styles.body} aria-busy={busy || loading}>
        <div className={styles.empty}>
          <h3>Tell us what you need to do</h3>
          <p>
            Describe it in Thai or English. ERPX can prepare a starting draft; you can also open a
            form directly.
          </p>
        </div>
        {result && (
          <section className={styles.intakeResult} aria-label="Prepared work draft">
            <p className={styles.intakeKind}>
              {result.intent === "unknown"
                ? "Choose a form"
                : `Suggested: ${actions.find((action) => action.id === result.intent)?.label ?? result.intent}`}
            </p>
            <p>{result.summary}</p>
            {!!fields.length && (
              <div className={styles.intakeFields}>
                {fields.map((field) => (
                  <Field appearance="standard" key={field}>
                    {field.replace(/([A-Z])/g, " $1").replace(/^./, (char) => char.toUpperCase())}
                    {["description", "body", "reason", "businessPurpose"].includes(field) ? (
                      <TextArea appearance="standard"
                        value={draft[field] ?? ""}
                        maxLength={3000}
                        onChange={(event) =>
                          setDraft((current) => ({ ...current, [field]: event.target.value }))
                        }
                        rows={3}
                      />
                    ) : (
                      <TextInput appearance="standard"
                        value={draft[field] ?? ""}
                        maxLength={300}
                        onChange={(event) =>
                          setDraft((current) => ({ ...current, [field]: event.target.value }))
                        }
                      />
                    )}
                  </Field>
                ))}
              </div>
            )}
            {!!intakeMissing(result.intent, draft).length && (
              <div className={styles.intakeMissing}>
                <strong>Check in the form</strong>
                <ul>
                  {intakeMissing(result.intent, draft).map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              </div>
            )}
            <div className={styles.sources}>
              {result.sources.map((source) =>
                source.path ? (
                  <Link
                    key={source.id}
                    href={`${base}${source.path}`}
                    title={`Source version ${source.version}`}
                  >
                    {source.title}
                  </Link>
                ) : (
                  <span key={source.id}>{source.title}</span>
                ),
              )}
            </div>
            {result.intent !== "unknown" &&
              actions.some((action) => action.id === result.intent) && (
                <UiActionLink appearance="standard"
                  variant="primary"
                  className={styles.continue}
                  href={actions.find((action) => action.id === result.intent)!.href}
                  onClick={(event) => {
                    if (result.intent !== "unknown") {
                      if (
                        !saveGuidedIntakeHandoff(
                          companyId,
                          userId,
                          assignmentId,
                          base,
                          result.intent,
                          draft,
                        )
                      ) {
                        event.preventDefault();
                        setError(
                          "This browser could not save the prepared details. Copy them from this panel or open a form directly below; your draft is still here.",
                        );
                        return;
                      }
                      window.dispatchEvent(new Event("erpx:guided-intake-handoff"));
                    }
                    onContinue();
                  }}
                >
                  Review in {actions.find((action) => action.id === result.intent)!.label} →
                </UiActionLink>
              )}
          </section>
        )}
        {unavailable && (
          <InlineNotice tone="info" className={styles.notice} role="status">
            {unavailable}
          </InlineNotice>
        )}
        {error && (
          <InlineNotice tone="danger" className={styles.notice} role="alert">
            {error}
          </InlineNotice>
        )}
        <div className={styles.manualActions}>
          <strong>Open a form directly</strong>
          {loading ? (
            <p>Loading available forms…</p>
          ) : (
            <div>
              {actions.map((action) => (
                <Link key={action.id} href={action.href}>
                  {action.label} →
                </Link>
              ))}
            </div>
          )}
        </div>
      </div>
      <form
        className={styles.composer}
        onSubmit={(event) => {
          event.preventDefault();
          void prepare(question);
        }}
      >
        <div className={styles.input}>
          <TextArea appearance="standard"
            aria-label="Describe work to prepare"
            placeholder="For example, I need two monitors for the design team…"
            value={question}
            maxLength={4000}
            onChange={(event) => setQuestion(event.target.value)}
            rows={2}
          />
          <Button appearance="standard"
            clientManaged type="submit"
            variant="ghost"
            aria-label="Prepare work draft"
            disabled={busy || loading || !!unavailable || question.trim().length < 3}
          >
            {busy ? <LoaderCircle className={styles.spin} size={17} /> : <Send size={17} />}
          </Button>
        </div>
        <small>AI only prepares. You review and confirm in the owning form.</small>
      </form>
    </>
  );
}

function AssistantConversation({
  base,
  path,
  mode,
  contextPaths,
  initialQuestion,
}: {
  base: string;
  path: string;
  mode: "page" | "workspace";
  contextPaths: string[];
  initialQuestion: string;
}) {
  const [loadedView, setView] = useState<AssistantView | null>(null),
    [question, setQuestion] = useState(initialQuestion),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [loading, setLoading] = useState(true);
  const current = useRef(path),
    generation = useRef(0),
    mounted = useRef(true),
    tail = useRef<HTMLDivElement>(null);
  const guide = assistantPage(path).guide;
  const view = loadedView?.page === path ? loadedView : null;
  const pending = loading || !view;
  const load = useCallback(
    async (signal?: AbortSignal) => {
      const response = await workFetch(
        `/api/v1${base}/assistant?${new URLSearchParams({ path, mode })}`,
        { cache: "no-store", signal },
      );
      const body = await response.json();
      if (!response.ok) throw Error(body.error?.message ?? "Assistant unavailable.");
      return body.data as AssistantView;
    },
    [base, path, mode],
  );
  useEffect(() => {
    mounted.current = true;
    current.current = path;
    const token = ++generation.current,
      abort = new AbortController();
    void load(abort.signal)
      .then((v) => {
        if (token === generation.current) {
          setView(v);
          setError("");
        }
      })
      .catch((e) => {
        if (!abort.signal.aborted)
          setError(e instanceof Error ? e.message : "Assistant unavailable.");
      })
      .finally(() => {
        if (!abort.signal.aborted) setLoading(false);
      });
    return () => {
      mounted.current = false;
      abort.abort();
    };
  }, [path, load]);
  useEffect(() => {
    tail.current?.scrollIntoView({ block: "nearest" });
  }, [view?.turns.length, busy]);
  async function send(text: string) {
    if (busy || !text.trim()) return;
    const sourcePath = path,
      token = generation.current;
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/assistant`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ path: sourcePath, mode, question: text, contextPaths }),
      });
      const body = await response.json();
      if (!response.ok)
        throw Error(body.error?.message ?? "AI could not respond. Retry or continue manually.");
      if (mounted.current && current.current === sourcePath && token === generation.current) {
        const updated = await load();
        if (mounted.current && current.current === sourcePath && token === generation.current) {
          setQuestion("");
          setView(updated);
        }
      }
    } catch (e) {
      if (mounted.current && token === generation.current)
        setError(e instanceof Error ? e.message : "AI is unavailable.");
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <>
      <div className={styles.body} aria-busy={busy || pending}>
        {!view?.turns.length && (
          <div className={styles.empty}>
            <h3>What would help you move forward?</h3>
            <p>
              Ask about {guide.title.toLowerCase()}, explain a step, or check what to prepare.
              Record details are included only where a supported, authorized source is available.
            </p>
            <div className={styles.prompts}>
              {guide.prompts.map((p) => (
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  key={p}
                  disabled={pending || busy || !!view?.unavailable || !view}
                  onClick={() => {
                    setQuestion(p);
                    void send(p);
                  }}
                >
                  {p} →
                </Button>
              ))}
            </div>
          </div>
        )}
        {!!view?.hiddenTurns && (
          <p className={styles.notice}>
            Some earlier answers are hidden because their sources or your access changed.
          </p>
        )}
        {view?.turns.map((t) => (
          <article key={t.id} className={styles.turn}>
            <div className={styles.question}>{t.question}</div>
            <div className={styles.answer}>{t.answer.answer}</div>
            <div className={styles.sources}>
              {t.sources
                .filter((s) => t.answer.sourceIds.includes(s.id))
                .map((s) => (
                  <Link
                    key={`${s.path}:${s.id}`}
                    href={`${base}${s.path}`}
                    title={`Source version ${s.version}`}
                  >
                    {s.title} · source {s.version.slice(0, 8)}
                  </Link>
                ))}
            </div>
            {t.answer.followUps.length > 0 && (
              <div className={styles.followUps}>
                <strong>Review next</strong>
                <ul>
                  {t.answer.followUps.map((step, i) => (
                    <li key={`${t.id}:${i}`}>{step}</li>
                  ))}
                </ul>
              </div>
            )}
          </article>
        ))}
        {view?.unavailable && (
          <InlineNotice tone="info" role="status" className={styles.notice}>
            {view.unavailable}
          </InlineNotice>
        )}
        {error && (
          <InlineNotice tone="danger" role="alert" className={styles.notice}>
            {error}
          </InlineNotice>
        )}
        {pending && !error && (
          <p className={styles.busy}>
            <LoaderCircle className={styles.spin} size={15} />
            Loading this conversation…
          </p>
        )}
        {busy && (
          <InlineNotice tone="info" role="status" className={styles.busy}>
            <LoaderCircle className={styles.spin} size={15} />
            Preparing an answer for this context…
          </InlineNotice>
        )}
        <div ref={tail} />
      </div>
      <form
        className={styles.composer}
        onSubmit={(e) => {
          e.preventDefault();
          void send(question);
        }}
      >
        <div className={styles.input}>
          <TextArea appearance="standard"
            aria-label="Message to work assistant"
            placeholder="Ask about this page…"
            value={question}
            maxLength={4000}
            onChange={(e) => setQuestion(e.target.value)}
            rows={2}
          />
          <Button appearance="standard"
            clientManaged type="submit"
            variant="ghost"
            aria-label="Send to assistant"
            disabled={busy || pending || !view || !!view.unavailable || !question.trim()}
          >
            <Send size={17} />
          </Button>
        </div>
        <small>AI helps prepare. You review and submit the work.</small>
      </form>
    </>
  );
}
