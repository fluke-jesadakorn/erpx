"use client";
import { Card } from "@/components/ui/foundation";

import { InlineNotice } from "@/components/ui/foundation";

import { ActionAnchor as UiActionAnchor } from "@/components/ui/foundation";

import { Button } from "@/components/ui/foundation";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import { currentStripeOnboardingUrl } from "@/lib/billing/company-stripe";
import styles from "./company-stripe.module.css";

type Connection = {
  account_id: string | null;
  state: string;
  livemode: boolean;
  charges_enabled: boolean;
  payouts_enabled: boolean;
  promptpay_enabled: boolean;
  thb_bank_ready: boolean;
  payout_interval: string | null;
  requirements_json: string;
};
type Job = {
  id: string;
  kind: string;
  state: string;
  last_error: string | null;
  payload_json: string;
};
type Requirements = { currently_due: string[]; pending_verification: string[] };
type SetupCommand = {
  action: "create" | "refresh" | "oauth";
  livemode: boolean;
  key: string;
  body: string;
  scope: string;
};

function storedCommand(storageKey: string): SetupCommand | null {
  try {
    const value: unknown = JSON.parse(sessionStorage.getItem(storageKey) ?? "null");
    if (value && typeof value === "object") {
      const command = value as Record<string, unknown>;
      if (
        command.scope === storageKey &&
        ["create", "refresh", "oauth"].includes(String(command.action)) &&
        typeof command.livemode === "boolean" &&
        typeof command.key === "string" &&
        command.key &&
        command.body === JSON.stringify({ livemode: command.livemode })
      )
        return command as SetupCommand;
    }
  } catch {}
  return null;
}

function requirementsFrom(value: string): Requirements {
  try {
    const parsed: unknown = JSON.parse(value || "[]");
    if (Array.isArray(parsed))
      return {
        currently_due: parsed.filter((item): item is string => typeof item === "string"),
        pending_verification: [],
      };
    if (parsed && typeof parsed === "object") {
      const record = parsed as Record<string, unknown>;
      return {
        currently_due: Array.isArray(record.currently_due)
          ? record.currently_due.filter((item): item is string => typeof item === "string")
          : [],
        pending_verification: Array.isArray(record.pending_verification)
          ? record.pending_verification.filter((item): item is string => typeof item === "string")
          : [],
      };
    }
  } catch {}
  return { currently_due: [], pending_verification: [] };
}

function requirementLabel(value: string, pending: boolean) {
  if (value.includes("verification.document"))
    return pending
      ? "Stripe is reviewing a verification document."
      : "Upload a verification document in Stripe.";
  if (value.startsWith("tos_acceptance."))
    return "Accept the connected account agreement in Stripe.";
  if (value.includes("external_account")) return "Add a THB bank account in Stripe.";
  return pending
    ? "Stripe is reviewing submitted information."
    : "Stripe needs more account information.";
}

function StatusTile({ label, ready, detail }: { label: string; ready: boolean; detail: string }) {
  return (
    <div className={styles.tile}>
      <span className={styles.tileLabel}>{label}</span>
      <strong className={ready ? styles.good : styles.waiting}>
        <span className={styles.dot} aria-hidden="true" />
        {ready ? "Ready" : "Pending"}
      </strong>
      <small>{detail}</small>
    </div>
  );
}

export function CompanyStripe({
  base,
  commandScope,
  connection,
  jobs,
  liveEnabled,
}: {
  base: string;
  commandScope: string;
  connection: Connection | null;
  jobs: Job[];
  liveEnabled: boolean;
}) {
  const router = useRouter(),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const storageKey = `erpx:stripe-setup:${commandScope}`,
    commandRef = useRef<SetupCommand | null>(null),
    [pendingCommand, setPendingCommand] = useState<SetupCommand | null>(null);
  useEffect(() => {
    queueMicrotask(() => {
      const saved = storedCommand(storageKey);
      commandRef.current = saved;
      setPendingCommand(saved);
    });
  }, [storageKey]);
  useEffect(() => {
    const timer = window.setInterval(() => router.refresh(), 5000);
    return () => window.clearInterval(timer);
  }, [router]);
  async function connect(action: SetupCommand["action"], livemode: boolean) {
    const previous =
      commandRef.current?.scope === storageKey ? commandRef.current : storedCommand(storageKey);
    if (previous && (previous.action !== action || previous.livemode !== livemode)) {
      setError("Retry the unfinished Stripe setup before starting another action.");
      return;
    }
    const command = previous ?? {
      action,
      livemode,
      key: crypto.randomUUID(),
      body: JSON.stringify({ livemode }),
      scope: storageKey,
    };
    commandRef.current = command;
    setPendingCommand(command);
    try {
      sessionStorage.setItem(storageKey, JSON.stringify(command));
    } catch {
      commandRef.current = null;
      setPendingCommand(null);
      setError("Browser session storage is unavailable. Enable it before starting Stripe setup.");
      return;
    }
    setBusy(true);
    setError("");
    const clear = () => {
      commandRef.current = null;
      setPendingCommand(null);
      try {
        sessionStorage.removeItem(storageKey);
      } catch {}
    };
    try {
      const response = await workFetch(`/api/v1${base}/pos/connection/${command.action}`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": command.key },
        body: command.body,
      });
      const body = await response.json();
      if (!response.ok) {
        if (response.status < 500 && response.status !== 408) clear();
        throw new Error(body.error?.message ?? "Stripe setup failed");
      }
      if (typeof body.data?.id !== "string")
        throw new Error("Stripe setup result could not be confirmed. Retry unfinished setup.");
      clear();
      if (body.data?.url) window.location.assign(body.data.url);
      else router.refresh();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Stripe setup failed");
    } finally {
      setBusy(false);
    }
  }

  const connected = Boolean(connection?.account_id),
    requirements = requirementsFrom(connection?.requirements_json ?? "[]");
  const paymentsReady = Boolean(connection?.charges_enabled && connection.promptpay_enabled);
  const payoutReady = Boolean(
    connection?.payouts_enabled &&
    connection.thb_bank_ready &&
    connection.payout_interval === "daily",
  );
  const complete = paymentsReady && payoutReady;
  const onboardingUrl = currentStripeOnboardingUrl(connection, jobs);
  const latestError = jobs[0]?.state === "failed" ? jobs[0].last_error : null;
  const due = [...new Set(requirements.currently_due.map((item) => requirementLabel(item, false)))];
  const pending = [
    ...new Set(requirements.pending_verification.map((item) => requirementLabel(item, true))),
  ];

  return (
    <Card as="section" appearance="standard" className={styles.panel} aria-labelledby="stripe-account-title">
      <div className={styles.header}>
        <div>
          <div className={styles.eyebrow}>
            Company payment account{" "}
            <span className={styles.mode}>{connection?.livemode ? "Live" : "Sandbox"}</span>
          </div>
          <h2 id="stripe-account-title">Stripe Connect</h2>
          <p>One connected account for this company’s POS, hotel payments and online bookings.</p>
        </div>
        {connected && (
          <Button appearance="standard"
            type="submit"
            variant="secondary"
            disabled={busy}
            onClick={() => void connect("refresh", Boolean(connection?.livemode))}
          >
            {busy ? "Checking…" : "Check Stripe status"}
          </Button>
        )}
      </div>

      {error && (
        <InlineNotice tone="danger" className={styles.error} role="alert">
          {error}
        </InlineNotice>
      )}
      {pendingCommand && (
        <InlineNotice tone="info" role="status">
          <p>A Stripe setup request is awaiting confirmation. Retry it to recover the result.</p>
          <Button appearance="standard"
            type="submit"
            variant="secondary"
            disabled={busy}
            onClick={() => void connect(pendingCommand.action, pendingCommand.livemode)}
          >
            Retry unfinished setup
          </Button>
        </InlineNotice>
      )}
      {latestError && !error && (
        <InlineNotice tone="danger" className={styles.error} role="alert">
          Stripe sync: {latestError}
        </InlineNotice>
      )}

      {connected ? (
        <>
          <InlineNotice tone="info"
            className={`${styles.summary} ${complete ? styles.summaryReady : paymentsReady ? styles.summaryPartial : styles.summaryWaiting}`}
            role="status"
          >
            <span className={styles.summaryIcon} aria-hidden="true">
              {complete ? "✓" : paymentsReady ? "◷" : "!"}
            </span>
            <div>
              <strong>
                {complete
                  ? "Payments and payouts are ready"
                  : paymentsReady
                    ? "PromptPay is ready for payments"
                    : "Account setup is in progress"}
              </strong>
              <p>
                {complete
                  ? "Stripe will pay the available THB balance on its daily schedule."
                  : paymentsReady
                    ? "Payouts are not ready yet. Review the account requirements below."
                    : "Complete the account requirements in Stripe before taking payments."}
              </p>
            </div>
          </InlineNotice>
          <div className={styles.accountLine}>
            <span>Connected account</span>
            <code>{connection?.account_id}</code>
          </div>
          <div className={styles.tiles}>
            <StatusTile
              label="PromptPay"
              ready={paymentsReady}
              detail="Direct charges for this company"
            />
            <StatusTile
              label="Payouts"
              ready={Boolean(connection?.payouts_enabled)}
              detail="Stripe to company bank"
            />
            <StatusTile
              label="THB bank"
              ready={Boolean(connection?.thb_bank_ready)}
              detail="Bank account on file"
            />
            <StatusTile
              label="Daily schedule"
              ready={connection?.payout_interval === "daily"}
              detail="Available balance only"
            />
          </div>
          {(due.length > 0 || pending.length > 0) && (
            <div className={styles.requirements}>
              <h3>{due.length ? "Action needed in Stripe" : "Verification in progress"}</h3>
              {due.length > 0 && (
                <ul>
                  {due.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              )}
              {pending.length > 0 && (
                <ul>
                  {pending.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
              )}
              <p>Stripe controls the review. Refresh this page after its account status changes.</p>
              {due.length > 0 && onboardingUrl && (
                <UiActionAnchor appearance="standard" variant="primary" href={onboardingUrl}>
                  Continue in Stripe
                </UiActionAnchor>
              )}
              {due.length > 0 && !onboardingUrl && (
                <p>Check Stripe status to get a current setup link for this account.</p>
              )}
            </div>
          )}
          {!complete && due.length === 0 && pending.length === 0 && (
            <p className={styles.note}>
              Stripe has no outstanding requirements in this snapshot. Check Stripe status to update
              payment and payout readiness.
            </p>
          )}
          <div className={styles.footer}>
            <p>
              Finance reconciles each payout after Stripe sends it. Payout date can differ from the
              POS or hotel business date.
            </p>
          </div>
          <details className={styles.advanced}>
            <summary>Account options and activity</summary>
            <div className={styles.advancedBody}>
              <p>Connect another account only when this company is changing its Stripe account.</p>
              <div className={styles.actions}>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void connect("oauth", false)}
                >
                  Connect existing test account
                </Button>
                {liveEnabled && (
                  <>
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => void connect("create", true)}
                    >
                      Set up live account
                    </Button>
                    <Button appearance="standard"
                      type="submit"
                      variant="secondary"
                      disabled={busy}
                      onClick={() => void connect("oauth", true)}
                    >
                      Connect existing live account
                    </Button>
                  </>
                )}
              </div>
              {jobs.length > 0 && (
                <ul className={styles.activity}>
                  {jobs.slice(0, 5).map((job) => (
                    <li key={job.id}>
                      <span>{job.kind === "connect_refresh" ? "Status check" : "Connection"}</span>
                      <strong>{job.state}</strong>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </details>
        </>
      ) : (
        <div className={styles.empty}>
          <h3>Connect a Stripe account</h3>
          <p>
            Set up a company account in Stripe sandbox to try PromptPay payments and payout
            reporting.
          </p>
          <div className={styles.actions}>
            <Button appearance="standard"
              type="submit"
              variant="primary"
              disabled={busy}
              onClick={() => void connect("create", false)}
            >
              Set up test account
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="secondary"
              disabled={busy}
              onClick={() => void connect("oauth", false)}
            >
              Connect existing test account
            </Button>
            {liveEnabled && (
              <>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void connect("create", true)}
                >
                  Set up live account
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="secondary"
                  disabled={busy}
                  onClick={() => void connect("oauth", true)}
                >
                  Connect existing live account
                </Button>
              </>
            )}
          </div>
        </div>
      )}
    </Card>
  );
}
