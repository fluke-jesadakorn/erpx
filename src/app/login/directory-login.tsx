"use client";
import { useState } from "react";
import { useRouter } from "next/navigation";
export function DirectoryLogin({
  id,
  name,
  mode,
  next,
}: {
  id: string;
  name: string;
  mode: string;
  next: string;
}) {
  const [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    router = useRouter();
  async function signIn(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const f = new FormData(e.currentTarget);
    setPending(true);
    setError("");
    try {
      const response = await fetch(
        mode === "oidc"
          ? "/api/auth/sign-in/social"
          : "/api/auth/directory/sign-in",
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(
            mode === "oidc"
              ? { provider: "directory-" + id, callbackURL: next }
              : {
                  connectionId: id,
                  username: f.get("username"),
                  password: f.get("password"),
                },
          ),
        },
      );
      const r = await response.json();
      if (!response.ok)
        throw new Error(r.message ?? "Directory sign-in failed.");
      if (r.url) window.location.assign(r.url);
      else {
        router.push(next);
        router.refresh();
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Sign-in failed.");
    } finally {
      setPending(false);
    }
  }
  return (
    <section className="card">
      <h2>{name}</h2>
      <p>Use your approved corporate identity.</p>
      <form className="form" onSubmit={signIn}>
        {mode === "ldaps" && (
          <>
            <label className="field">
              Directory username
              <input name="username" autoComplete="username" required />
            </label>
            <label className="field">
              Directory password
              <input
                name="password"
                type="password"
                autoComplete="current-password"
                required
              />
            </label>
          </>
        )}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
        <button className="button" disabled={pending}>
          {pending
            ? "Connecting…"
            : mode === "oidc"
              ? "Continue to corporate sign-in"
              : "Sign in with directory"}
        </button>
      </form>
    </section>
  );
}
