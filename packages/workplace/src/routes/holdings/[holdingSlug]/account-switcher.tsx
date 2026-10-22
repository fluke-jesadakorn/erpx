"use client";
import type {WorkRoleDisplayChoice} from "@erpx/feature-identity_access/lib/work-role-display";
import {accountSelfNavigation} from '@erpx/feature-identity_access/lib/account-self-navigation';
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";

import { Button, Field, TextInput, Popover, PopoverTrigger, PopoverContent, type UiAppearance } from "@erpx/ui/components/foundation";

import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { RoleSelect } from "@erpx/workplace/components/role-select";
import { PersonAvatar } from "@erpx/ui/components/person-avatar";
import { useHydrated } from "@erpx/ui/components/use-hydrated";

import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, LogIn, LogOut, Plus, UserRound } from "lucide-react";
import styles from "@erpx/workplace/components/account.module.css";
import { Appearance } from "@erpx/workplace/components/workplace-controls";
import { leavePushSession } from "@erpx/feature-shared/lib/push-client";
import { useContextSwitch } from "@erpx/workplace/components/context-switch-provider";
import {
  confirmContextSwitch,
  contextSwitchBlockReason,
  setContextSwitching,
} from "@erpx/feature-work_projection/lib/context-switch";

type DeviceSession = {
  session: { token: string; userId: string };
  user: { id: string; name: string; email: string; image?: string | null };
};

type Props = {
  accountNavigation?: boolean;
  appearance?: UiAppearance;
  assignments?: readonly WorkRoleDisplayChoice[];
  preferredAssignmentId?: string | null;
  contextRevision?: number;
  companyId?: string;
  workspaceBase: string;
  activeUser: { id: string; name: string; email: string };
  theme: string;
};

export function AccountSwitcher({
  accountNavigation = false,
  appearance = "legacy",
  workspaceBase,
  activeUser,
  theme,
  assignments = [],
  preferredAssignmentId = null,
  contextRevision = 0,
  companyId,
}: Props) {
  const selfNavigation=accountSelfNavigation(workspaceBase,activeUser.id,accountNavigation);
  const { switchContext, busy } = useContextSwitch();
  const ready = useHydrated();
  const root = useRef<HTMLDivElement>(null);
  const scrollPosition = useRef({ x: 0, y: 0 });
  const [open, setOpen] = useState(false);
  const [adding, setAdding] = useState(false);
  const [pending, setPending] = useState("");
  const [error, setError] = useState("");
  const [sessions, setSessions] = useState<DeviceSession[]>([]);

  async function loadAccounts() {
    const { authClient } = await import("@erpx/feature-identity_access/lib/auth-client");
    const result = await authClient.multiSession.listDeviceSessions();
    if (result.data) setSessions(result.data as DeviceSession[]);
  }

  useEffect(() => {
    if (appearance === "standard") return;
    function close(event: MouseEvent) {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [appearance]);

  useEffect(() => {
    if (!open || appearance === "standard") return;
    const previous = document.activeElement as HTMLElement | null;
    const menu = root.current?.querySelector<HTMLElement>('[role="dialog"]');
    const focusable = () =>
      Array.from(
        menu?.querySelectorAll<HTMLElement>(
          "button:not(:disabled),input:not(:disabled),a[href],select",
        ) ?? [],
      ).filter((element) => element.getClientRects().length);
    focusable()[0]?.focus({ preventScroll: true });
    function keyboard(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
      }
      if (event.key === "Tab") {
        const items = focusable(),
          first = items[0],
          last = items.at(-1);
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first?.focus();
        }
      }
    }
    document.addEventListener("keydown", keyboard);
    return () => {
      document.removeEventListener("keydown", keyboard);
      previous?.focus({ preventScroll: true });
    };
  }, [open, adding, appearance]);

  async function chooseAccount(session: DeviceSession) {
    if (session.user.id === activeUser.id || pending || busy) return;
    setPending(`account:${session.user.id}`);
    setError("");
    try {
      await switchContext({
        companyId,
        scroll: scrollPosition.current,
        onCommitted: () => setOpen(false),
        action: async () => {
          if (workspaceBase !== "/platform") await leavePushSession(workspaceBase);
          const { authClient } = await import("@erpx/feature-identity_access/lib/auth-client");
          const result = await authClient.multiSession.setActive({
            sessionToken: session.session.token,
          });
          if (result.error)
            throw new Error(result.error.message ?? "Account could not be selected.");
        },
      });
    } catch (error) {
      setError(error instanceof Error ? error.message : "Account could not be selected.");
    } finally {
      setPending("");
    }
  }

  async function addAccount(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending("add");
    setError("");
    const form = new FormData(event.currentTarget);
    try {
      await switchContext({
        companyId,
        scroll: scrollPosition.current,
        onCommitted: () => setOpen(false),
        action: async () => {
          if (workspaceBase !== "/platform") await leavePushSession(workspaceBase);
          const { authClient } = await import("@erpx/feature-identity_access/lib/auth-client");
          const result = await authClient.signIn.email({
            email: String(form.get("email")),
            password: String(form.get("password")),
          });
          if (result.error) throw new Error(result.error.message ?? "Account could not be added.");
        },
      });
    } catch (error) {
      setError(error instanceof Error ? error.message : "Account could not be added.");
    } finally {
      setPending("");
    }
  }

  async function chooseRole(assignmentId: string) {
    if (assignmentId === preferredAssignmentId || pending || busy) return;
    setPending("role");
    setError("");
    try {
      await switchContext({
        companyId,
        scroll: scrollPosition.current,
        onCommitted: () => setOpen(false),
        action: async () => {
          const response = await workFetch(`/api/v1${workspaceBase}/assignments/active`, {
            method: "POST",
            headers: {
              "content-type": "application/json",
              "x-erpx-context-company":
                assignments.find((option) => option.id === assignmentId)?.companyId ??
                companyId ??
                "",
            },
            body: JSON.stringify({ assignmentId, revision: contextRevision }),
          });
          const body = await response.json();
          if (!response.ok)
            throw new Error(
              body.error?.message ??
                "Role could not be selected. Refresh to check your current access.",
            );
        },
      });
    } catch (error) {
      setError(error instanceof Error ? error.message : "Role could not be selected.");
    } finally {
      setPending("");
    }
  }

  async function signOut() {
    const blockedReason = contextSwitchBlockReason();
    if (blockedReason) {
      setError(blockedReason);
      return;
    }
    if (!confirmContextSwitch("Sign out and discard unsaved changes?")) return;
    setError("");
    setPending("logout");
    setContextSwitching(true);
    try {
      try {
        if (workspaceBase !== "/platform") await leavePushSession(workspaceBase);
      } catch {
        /* Revoking the auth session also blocks future sends. */
      }
      const { authClient } = await import("@erpx/feature-identity_access/lib/auth-client");
      await authClient.signOut();
      window.location.replace(new URL("/login", window.location.origin));
    } catch (cause) {
      setContextSwitching(false);
      setPending("");
      setError(cause instanceof Error ? cause.message : "Could not sign out. Try again.");
    }
  }

  const accounts = sessions.some((item) => item.user.id === activeUser.id)
    ? sessions
    : [{ session: { token: "", userId: activeUser.id }, user: activeUser }, ...sessions];

  const trigger = (      <Button appearance={appearance}
        variant="plain"
        className="account-trigger"
        disabled={!ready || busy || Boolean(pending)}
        aria-label={`Account menu for ${activeUser.name}, ${activeUser.email}`}
        title={`${activeUser.name} · ${activeUser.email}`}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          const next = !open;
          if (next) scrollPosition.current = { x: window.scrollX, y: window.scrollY };
          setOpen(next);
          setAdding(false);
          setError("");
          if (next) void loadAccounts();
        }}
      >
        <PersonAvatar name={activeUser.name} small />
        <span className="user-copy">
          <strong>{activeUser.name}</strong>
          <span
            className="active-work-role"
            title={assignments.find((a) => a.id === preferredAssignmentId)?.label}
          >
            {assignments.find((a) => a.id === preferredAssignmentId)?.label ?? activeUser.email}
          </span>
        </span>
        <ChevronDown size={15} />
      </Button>);
  const content = <>          <div className="account-menu-head">
            <div>
              <span className="eyebrow">Signed in accounts</span>
              <strong>Account & work role</strong>
            </div>
            <span className="session-count">{accounts.length}/5</span>
          </div>
          {!adding && (
            <>
              <div className="account-list">
                {accounts.map((item) => {
                  const active = item.user.id === activeUser.id;
                  return (
                    <Button appearance={appearance}
                      variant="plain"
                      type="button"
                      className={`account-option ${active ? "active" : ""}`}
                      aria-current={active ? "true" : undefined}
                      key={item.user.id}
                      disabled={Boolean(pending)}
                      onClick={() => void chooseAccount(item)}
                    >
                      <PersonAvatar name={item.user.name} />
                      <span>
                        <strong>{item.user.name}</strong>
                        <small>{item.user.email}</small>
                      </span>
                      {active ? (
                        <Check size={17} />
                      ) : pending === `account:${item.user.id}` ? (
                        <span className="mini-spinner" />
                      ) : (
                        <LogIn size={16} />
                      )}
                    </Button>
                  );
                })}
              </div>
              <UiActionLink appearance={appearance}
                variant="ghost"
                href={selfNavigation.access}
                onClick={() => setOpen(false)}
              >
                My access & security
              </UiActionLink>
              {workspaceBase !== "/platform" && (
                <UiActionLink appearance={appearance}
                  variant="ghost"
                  href={`${workspaceBase}/settings/notifications`}
                  onClick={() => setOpen(false)}
                >
                  Notification settings
                </UiActionLink>
              )}
              {selfNavigation.profile&&<UiActionLink appearance={appearance}
                variant="ghost"
                href={selfNavigation.profile}
                onClick={() => setOpen(false)}
              >
                My profile
              </UiActionLink>}
              {selfNavigation.companyProfile && (
                <UiActionLink appearance={appearance}
                  variant="ghost"
                  href={selfNavigation.companyProfile}
                  onClick={() => setOpen(false)}
                >
                  Company profile
                </UiActionLink>
              )}
              {assignments.length > 0 && (
                <section className="role-section">
                  <RoleSelect appearance={appearance}
                    assignments={assignments}
                    value={preferredAssignmentId ?? ""}
                    disabled={Boolean(pending) || busy}
                    onChange={(value) => void chooseRole(value)}
                  />
                  <p className="role-hint">
                    Only this role is used for actions and approvals. Self-approval rules still
                    apply.
                  </p>
                </section>
              )}
              <div className="appearance-row">
                <span>Appearance</span>
                <Appearance appearance={appearance} initialTheme={theme} compact />
              </div>
              <div className="account-actions">
                <Button appearance={appearance}
                  variant="plain"
                  type="button"
                  onClick={() => {
                    setAdding(true);
                    setError("");
                  }}
                  disabled={accounts.length >= 5 || Boolean(pending) || busy}
                >
                  <Plus size={16} />
                  Add another account
                </Button>
                <Button appearance={appearance}
                  variant="plain"
                  type="button"
                  disabled={Boolean(pending) || busy}
                  onClick={() => void signOut()}
                >
                  <LogOut size={16} />
                  Sign out all
                </Button>
              </div>
            </>
          )}
          {adding && (
            <form className="add-account-form" onSubmit={addAccount}>
              <div className="add-account-title">
                <span>
                  <UserRound size={18} />
                </span>
                <div>
                  <strong>Add an existing account</strong>
                  <small>Verify its credentials once to keep it available on this device.</small>
                </div>
              </div>
              <Field>
                Work email
                <TextInput appearance={appearance} name="email" type="email" autoComplete="username" required autoFocus />
              </Field>
              <Field>
                Password
                <TextInput appearance={appearance}
                  name="password"
                  type="password"
                  autoComplete="current-password"
                  minLength={12}
                  required
                />
              </Field>
              <div className="add-account-actions">
                <Button appearance={appearance}
                  variant="secondary"
                  type="button"

                  onClick={() => setAdding(false)}
                >
                  Back
                </Button>
                <Button appearance={appearance} clientManaged type="submit" variant="primary" disabled={pending === "add"}>
                  {pending === "add" ? "Adding…" : "Add account"}
                </Button>
              </div>
            </form>
          )}
          {error && (
            <div className="account-error" role="alert">
              {error}
            </div>
          )}
</>;
  return <div className={`account-switcher ${styles.skin}`} ref={root}>
    {appearance === "standard" ? <Popover open={open} modal onOpenChange={next => setOpen(next)}>
      <PopoverTrigger render={trigger} />
      <PopoverContent className={`${styles.skin} ui-account-popup`} aria-label="Account switcher">{content}</PopoverContent>
    </Popover> : <>{trigger}{open && <div className="account-menu" role="dialog" aria-label="Account switcher">{content}</div>}</>}
  </div>;
}
