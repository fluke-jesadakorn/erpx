"use client";
import { Button, Field, TextInput } from "@erpx/ui/components/foundation";
import {holdingHostForHost,holdingTransportPath} from '@erpx/contracts/holding-hosts';
import {isProductHostKey} from '@erpx/contracts/product-registry';

import { useEffect, useState } from "react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import {
  applicationServerKey,
  currentPushSubscription,
  leavePushSession,
  supportsPush,
} from "@erpx/feature-shared/lib/push-client";
import styles from "@erpx/feature-platform_administration/components/settings/push-settings.module.css";
import { type UiAppearance } from "@erpx/ui/components/foundation";

type Preferences = {
  enabled: boolean;
  work_enabled: boolean;
  mention_enabled: boolean;
  system_enabled: boolean;
  calendar_enabled: boolean;
  notice_enabled: boolean;
  quiet_start_minute: number | null;
  quiet_end_minute: number | null;
};
type State = {
  available: boolean;
  publicKey: string | null;
  preferences: Preferences | null;
  deviceCount: number;
};
const empty: State = { available: false, publicKey: null, preferences: null, deviceCount: 0 };
const hhmm = (minute: number | null) =>
  minute === null
    ? ""
    : `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
const minute = (value: string) =>
  value ? Number(value.slice(0, 2)) * 60 + Number(value.slice(3, 5)) : null;

export function PushSettings({ appearance = "legacy",
  base,
  frontCompanySlug,
}: ({
  base: string;
  frontCompanySlug?: string;
}) & { appearance?: UiAppearance }) {
  const [state, setState] = useState<State>(empty),
    [permission, setPermission] = useState<NotificationPermission | "unsupported">("unsupported");
  const [subscribed, setSubscribed] = useState(false),
    [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const [draft, setDraft] = useState<Preferences | null>(null);
  useEffect(() => {
    let active = true;
    async function load() {
      try {
        const response = await workFetch(`/api/v1${base}/push`, { cache: "no-store" });
        const body = await response.json();
        if (!response.ok) throw new Error(body.error?.message ?? "Push settings are unavailable.");
        if (!active) return;
        setState(body.data);
        setDraft(body.data.preferences);
        if (supportsPush()) {
          setPermission(Notification.permission);
          const subscription = await currentPushSubscription();
          if (active) setSubscribed(Boolean(subscription));
          if (Notification.permission === "denied" && body.data.deviceCount > 0) {
            await workFetch(`/api/v1${base}/push`, {
              method: "DELETE",
              headers: { "Content-Type": "application/json" },
              body: "{}",
            });
          }
        }
      } catch (error) {
        if (active)
          setMessage(error instanceof Error ? error.message : "Push settings are unavailable.");
      } finally {
        if (active) setLoading(false);
      }
    }
    void load();
    return () => {
      active = false;
    };
  }, [base]);
  async function enable() {
    if (!state.publicKey || !supportsPush()) return;
    setBusy(true);
    setMessage("");
    let created: PushSubscription | null = null;
    try {
      const result = await Notification.requestPermission();
      setPermission(result);
      if (result !== "granted") {
        setMessage("Allow notifications in your browser settings to turn on push.");
        return;
      }
      const app=document.documentElement.dataset.erpxApp;
      const script=holdingHostForHost(window.location.host)&&app&&isProductHostKey(app)?holdingTransportPath(app,'/erpx-push-sw.js'):'/erpx-push-sw.js';
      const registration = await navigator.serviceWorker.register(script, {
        scope: "/",
      });
      let subscription = await registration.pushManager.getSubscription();
      const key = applicationServerKey(state.publicKey);
      if (
        subscription?.options.applicationServerKey &&
        (subscription.options.applicationServerKey.byteLength !== key.length ||
          !Array.from(new Uint8Array(subscription.options.applicationServerKey)).every(
            (byte, index) => byte === key[index],
          ))
      ) {
        await subscription.unsubscribe();
        subscription = null;
      }
      if (!subscription) {
        subscription = await registration.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: key,
        });
        created = subscription;
      }
      const response = await workFetch(`/api/v1${base}/push`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          action: "subscribe",
          subscription: subscription.toJSON(),
          frontCompanySlug,
        }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Could not enable push.");
      setSubscribed(true);
      setState((previous) => ({
        ...previous,
        deviceCount: 1,
        preferences: previous.preferences ? { ...previous.preferences, enabled: true } : null,
      }));
      setMessage("Push is on for this browser.");
    } catch (error) {
      if (created) await created.unsubscribe();
      setMessage(error instanceof Error ? error.message : "Could not enable push.");
    } finally {
      setBusy(false);
    }
  }
  async function disable() {
    setBusy(true);
    setMessage("");
    try {
      await leavePushSession(base);
      setSubscribed(false);
      setState((previous) => ({
        ...previous,
        deviceCount: 0,
        preferences: previous.preferences ? { ...previous.preferences, enabled: false } : null,
      }));
      setMessage("Push is off for this browser.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Could not turn off push.");
    } finally {
      setBusy(false);
    }
  }
  async function save() {
    if (!draft) return;
    setBusy(true);
    setMessage("");
    try {
      const response = await workFetch(`/api/v1${base}/push`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          action: "preferences",
          preferences: {
            work: draft.work_enabled,
            mention: draft.mention_enabled,
            system: draft.system_enabled,
            calendar: draft.calendar_enabled,
            notice: draft.notice_enabled,
            quietStartMinute: draft.quiet_start_minute,
            quietEndMinute: draft.quiet_end_minute,
          },
        }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Could not save preferences.");
      setState((previous) => ({ ...previous, preferences: draft }));
      setMessage("Preferences saved.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Could not save preferences.");
    } finally {
      setBusy(false);
    }
  }
  const setFlag = (key: keyof Preferences, value: boolean) =>
    setDraft((previous) => (previous ? { ...previous, [key]: value } : previous));
  return (
    <section data-ui-appearance={appearance} className={styles.root} aria-labelledby="push-heading">
      <h2 data-ui-appearance={appearance} id="push-heading">Web Push</h2>
      <p data-ui-appearance={appearance}>
        Get a brief alert when work needs you. Details stay in your Inbox and are checked again when
        opened.
      </p>
      {loading ? (
        <p data-ui-appearance={appearance} role="status">Loading push settings…</p>
      ) : !state.available ? (
        <p data-ui-appearance={appearance} role="status">
          Push is not configured for this workspace. Your in-app Inbox still works.
        </p>
      ) : permission === "unsupported" ? (
        <p data-ui-appearance={appearance} role="status">
          This browser or connection does not support Web Push. Your in-app Inbox still works.
        </p>
      ) : (
        <>
          <p data-ui-appearance={appearance} className={styles.status}>
            Browser: {permission}. This device: {subscribed && state.deviceCount > 0 ? "On" : "Off"}
            .
          </p>
          <div data-ui-appearance={appearance} className={styles.actions}>
            <Button appearance={appearance}
              variant="plain"
              type="button"
              disabled={busy || (subscribed && state.deviceCount > 0)}
              onClick={() => void enable()}
            >
              Turn on for this browser
            </Button>
            <Button appearance={appearance}
              variant="plain"
              type="button"
              disabled={busy || !(subscribed && state.deviceCount > 0)}
              onClick={() => void disable()}
            >
              Turn off for this browser
            </Button>
          </div>
          {draft && (
            <fieldset data-ui-appearance={appearance} className={styles.preferences} disabled={busy}>
              <legend data-ui-appearance={appearance}>Send alerts for</legend>
              {(
                [
                  ["work_enabled", "Work assigned to me"],
                  ["mention_enabled", "Mentions"],
                  ["system_enabled", "Important system updates"],
                  ["calendar_enabled", "Calendar reminders"],
                  ["notice_enabled", "Notices"],
                ] as const
              ).map(([key, label]) => (
                <Field appearance={appearance} orientation="inline" key={key}>
                  <TextInput appearance={appearance}
                    type="checkbox"
                    checked={draft[key]}
                    onChange={(event) => setFlag(key, event.target.checked)}
                  />
                  {label}
                </Field>
              ))}
              <div data-ui-appearance={appearance} className={styles.quiet}>
                <span data-ui-appearance={appearance}>Quiet hours (Asia/Bangkok)</span>
                <Field appearance={appearance}>
                  From{" "}
                  <TextInput appearance={appearance}
                    type="time"
                    value={hhmm(draft.quiet_start_minute)}
                    onChange={(event) =>
                      setDraft((previous) =>
                        previous
                          ? { ...previous, quiet_start_minute: minute(event.target.value) }
                          : previous,
                      )
                    }
                  />
                </Field>
                <Field appearance={appearance}>
                  To{" "}
                  <TextInput appearance={appearance}
                    type="time"
                    value={hhmm(draft.quiet_end_minute)}
                    onChange={(event) =>
                      setDraft((previous) =>
                        previous
                          ? { ...previous, quiet_end_minute: minute(event.target.value) }
                          : previous,
                      )
                    }
                  />
                </Field>
                <Button appearance={appearance}
                  variant="plain"
                  type="button"
                  onClick={() =>
                    setDraft((previous) =>
                      previous
                        ? { ...previous, quiet_start_minute: null, quiet_end_minute: null }
                        : previous,
                    )
                  }
                >
                  Clear quiet hours
                </Button>
              </div>
              <Button appearance={appearance} variant="plain" type="button" onClick={() => void save()}>
                Save preferences
              </Button>
            </fieldset>
          )}
        </>
      )}
      {message && (
        <p data-ui-appearance={appearance} role="status" className={styles.message}>
          {message}
        </p>
      )}
    </section>
  );
}
