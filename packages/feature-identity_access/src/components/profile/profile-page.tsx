"use client";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { Button, Field, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useEffect, useRef, useState } from "react";
import Link from "@erpx/workplace/components/workspace-link";
import Image from "next/image";
import { Camera, ExternalLink, Link2, MapPin, Pencil, UserRound } from "lucide-react";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import { TimePreferenceEditor } from "@erpx/feature-people_payroll/components/time/time-preference-editor";
import type { memberProfileView } from "@erpx/server-organization/public/member-profiles";
import styles from "@erpx/feature-identity_access/components/profile/profile-page.module.css";

type Profile = Awaited<ReturnType<typeof memberProfileView>>;

export function ProfilePage({ initial, base }: { initial: Profile; base: string }) {
  const [profile, setProfile] = useState(initial);
  const [tab, setTab] = useState<"posts" | "about">("posts");
  const [editing, setEditing] = useState(false);
  const [bio, setBio] = useState(initial.bio);
  const [links, setLinks] = useState(initial.links);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [avatarPreview, setAvatarPreview] = useState<string | null>(null);
  const [coverPreview, setCoverPreview] = useState<string | null>(null);
  const [pendingImage, setPendingImage] = useState<{ kind: "avatar" | "cover"; file: File } | null>(
    null,
  );
  const [moreBusy, setMoreBusy] = useState(false);
  const avatarInput = useRef<HTMLInputElement>(null);
  const coverInput = useRef<HTMLInputElement>(null);
  useEffect(
    () => () => {
      if (avatarPreview) URL.revokeObjectURL(avatarPreview);
      if (coverPreview) URL.revokeObjectURL(coverPreview);
    },
    [avatarPreview, coverPreview],
  );
  const api = `/api/v1${base}/profiles`;
  async function save() {
    setBusy(true);
    setMessage("");
    try {
      const response = await workFetch(`${api}/me`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ bio, links, version: profile.version }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save profile.");
      setProfile((current) => ({
        ...current,
        bio: body.data.bio,
        links: body.data.links,
        version: body.data.version,
      }));
      setEditing(false);
      setMessage("Profile saved.");
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Unable to save profile.");
    } finally {
      setBusy(false);
    }
  }
  function stageImage(kind: "avatar" | "cover", file: File) {
    const max = kind === "avatar" ? 2 * 1024 * 1024 : 5 * 1024 * 1024;
    if (!["image/jpeg", "image/png", "image/webp"].includes(file.type) || file.size > max) {
      setMessage(`Choose a JPEG, PNG or WebP ${kind} under ${kind === "avatar" ? 2 : 5} MB.`);
      return;
    }
    const preview = URL.createObjectURL(file);
    setAvatarPreview(kind === "avatar" ? preview : null);
    setCoverPreview(kind === "cover" ? preview : null);
    setPendingImage({ kind, file });
    setEditing(true);
    setMessage("");
  }
  function discardImage() {
    setPendingImage(null);
    setAvatarPreview(null);
    setCoverPreview(null);
  }
  async function upload(kind: "avatar" | "cover", file: File) {
    setBusy(true);
    setMessage("");
    try {
      const response = await workFetch(`${api}/me/media/${kind}`, {
        method: "POST",
        headers: {
          "Content-Type": file.type,
          "x-profile-version": String(profile.version),
          "Idempotency-Key": crypto.randomUUID(),
        },
        body: file,
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to upload image.");
      setProfile((current) => ({
        ...current,
        version: body.data.version,
        [kind === "avatar" ? "avatarUrl" : "coverUrl"]:
          `${api}/${encodeURIComponent(current.userId)}/media/${kind}?v=${body.data.version}`,
      }));
      discardImage();
      setMessage(`${kind === "avatar" ? "Avatar" : "Cover"} updated.`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Unable to upload image.");
    } finally {
      setBusy(false);
    }
  }
  async function removeImage(kind: "avatar" | "cover") {
    setBusy(true);
    setMessage("");
    try {
      const response = await workFetch(`${api}/me/media/${kind}`, {
        method: "DELETE",
        headers: {
          "x-profile-version": String(profile.version),
          "Idempotency-Key": crypto.randomUUID(),
        },
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to remove image.");
      setProfile((current) => ({
        ...current,
        version: body.data.version,
        [kind === "avatar" ? "avatarUrl" : "coverUrl"]: null,
      }));
      setMessage(`${kind === "avatar" ? "Avatar" : "Cover"} removed.`);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Unable to remove image.");
    } finally {
      setBusy(false);
    }
  }
  async function loadMore() {
    if (!profile.nextBefore || moreBusy) return;
    setMoreBusy(true);
    try {
      const response = await workFetch(
        `${api}/${encodeURIComponent(profile.userId)}?before=${profile.nextBefore}&beforeId=${encodeURIComponent(profile.nextBeforeId ?? "")}`,
        { cache: "no-store" },
      );
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to load posts.");
      setProfile((current) => ({
        ...current,
        posts: [
          ...current.posts,
          ...body.data.posts.filter(
            (post: Profile["posts"][number]) =>
              !current.posts.some((existing) => existing.id === post.id),
          ),
        ],
        nextBefore: body.data.nextBefore,
        nextBeforeId: body.data.nextBeforeId,
      }));
    } catch (error) {
      setMessage(error instanceof Error ? error.message : "Unable to load posts.");
    } finally {
      setMoreBusy(false);
    }
  }
  const picture = avatarPreview ?? profile.avatarUrl;
  const banner = coverPreview ?? profile.coverUrl;
  return (
    <div className={styles.page}>
      <Link className={styles.back} href={canonicalUiHref(`${base}/people`)}>
        ← People
      </Link>
      <section className={styles.hero}>
        <div className={styles.cover}>
          {banner && (
            <Image
              unoptimized
              src={banner}
              alt={`${profile.name}'s cover`}
              fill
              sizes="(max-width: 800px) 100vw, 1100px"
              className={styles.coverImage}
            />
          )}
          {profile.own && (
            <>
              <Button appearance="standard"
                variant="ghost"
                type="button"
                className={styles.coverEdit}
                disabled={busy}
                onClick={() => coverInput.current?.click()}
              >
                <Camera size={16} /> Change cover
              </Button>
              <TextInput appearance="standard"
                ref={coverInput}
                className={styles.hiddenFile}
                aria-label="Cover image file"
                tabIndex={-1}
                type="file"
                accept="image/jpeg,image/png,image/webp"
                disabled={busy}
                onChange={(event) => {
                  const file = event.target.files?.[0];
                  if (file) stageImage("cover", file);
                  event.target.value = "";
                }}
              />
            </>
          )}
        </div>
        <div className={styles.identity}>
          <div className={styles.avatar}>
            {picture ? (
              <Image
                unoptimized
                src={picture}
                alt={`${profile.name}'s avatar`}
                fill
                sizes="112px"
              />
            ) : (
              <UserRound size={50} aria-hidden="true" />
            )}
            {profile.own && (
              <>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  className={styles.avatarEdit}
                  aria-label="Change avatar"
                  disabled={busy}
                  onClick={() => avatarInput.current?.click()}
                >
                  <Camera size={16} />
                </Button>
                <TextInput appearance="standard"
                  ref={avatarInput}
                  className={styles.hiddenFile}
                  aria-label="Avatar image file"
                  tabIndex={-1}
                  type="file"
                  accept="image/jpeg,image/png,image/webp"
                  disabled={busy}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) stageImage("avatar", file);
                    event.target.value = "";
                  }}
                />
              </>
            )}
          </div>
          <div className={styles.identityText}>
            <span className={styles.eyebrow}>Member profile</span>
            <h1>{profile.name}</h1>
            {profile.work.length > 0 && (
              <p className={styles.role}>
                {profile.work[0].title || "Team member"} · {profile.work[0].company}
              </p>
            )}
            {profile.bio && <p className={styles.bio}>{profile.bio}</p>}
            {profile.links.length > 0 && (
              <div className={styles.links}>
                {profile.links.map((link) => (
                  <a key={link.url} href={link.url} target="_blank" rel="noopener noreferrer">
                    <Link2 size={14} /> {link.label} <ExternalLink size={12} />
                  </a>
                ))}
              </div>
            )}
          </div>
          {profile.own && (
            <Button appearance="standard"
              variant="ghost"
              type="button"
              className={styles.editButton}
              onClick={() => {
                setBio(profile.bio);
                setLinks(profile.links);
                if (editing) discardImage();
                setEditing((value) => !value);
              }}
            >
              <Pencil size={16} /> {editing ? "Close editor" : "Edit profile"}
            </Button>
          )}
        </div>
      </section>
      {message && (
        <InlineNotice tone="info" className={styles.message} role="status">
          {message}
        </InlineNotice>
      )}
      {editing && profile.own && (
        <section className={styles.editor} aria-label="Edit your profile">
          <h2>Make it yours</h2>
          {pendingImage && (
            <InlineNotice tone="info" className={`${styles.imageActions} ${styles.imagePreview}`} role="status">
              <p>Previewing {pendingImage.kind}. Review the image above before saving.</p>
              <Button appearance="standard"
                variant="primary"
                type="button"
                disabled={busy}
                onClick={() => void upload(pendingImage.kind, pendingImage.file)}
              >
                {busy ? "Saving…" : `Save ${pendingImage.kind}`}
              </Button>
              <Button appearance="standard" variant="ghost" type="button" disabled={busy} onClick={discardImage}>
                Cancel image
              </Button>
            </InlineNotice>
          )}
          <div className={styles.imageActions}>
            {profile.avatarUrl && (
              <Button appearance="standard"
                variant="ghost"
                type="button"
                disabled={busy}
                onClick={() => void removeImage("avatar")}
              >
                Remove avatar
              </Button>
            )}
            {profile.coverUrl && (
              <Button appearance="standard"
                variant="ghost"
                type="button"
                disabled={busy}
                onClick={() => void removeImage("cover")}
              >
                Remove cover
              </Button>
            )}
          </div>
          <Field appearance="standard">
            About you
            <TextArea appearance="standard"
              maxLength={500}
              rows={4}
              value={bio}
              onChange={(event) => setBio(event.target.value)}
              placeholder="Tell your colleagues what you work on…"
            />
            <small>{bio.length}/500</small>
          </Field>
          <h3>Links</h3>
          {links.map((link, index) => (
            <div className={styles.linkRow} key={index}>
              <TextInput appearance="standard"
                aria-label={`Link ${index + 1} label`}
                maxLength={40}
                value={link.label}
                placeholder="Label"
                onChange={(event) =>
                  setLinks((current) =>
                    current.map((item, i) =>
                      i === index ? { ...item, label: event.target.value } : item,
                    ),
                  )
                }
              />
              <TextInput appearance="standard"
                aria-label={`Link ${index + 1} URL`}
                type="url"
                value={link.url}
                placeholder="https://"
                onChange={(event) =>
                  setLinks((current) =>
                    current.map((item, i) =>
                      i === index ? { ...item, url: event.target.value } : item,
                    ),
                  )
                }
              />
              <Button appearance="standard"
                variant="ghost"
                type="button"
                onClick={() => setLinks((current) => current.filter((_, i) => i !== index))}
              >
                Remove
              </Button>
            </div>
          ))}
          {links.length < 3 && (
            <Button appearance="standard"
              variant="ghost"
              type="button"
              className={styles.addLink}
              onClick={() => setLinks((current) => [...current, { label: "", url: "" }])}
            >
              + Add a link
            </Button>
          )}
          <div>
            <Button appearance="standard"
              variant="primary"
              type="button"
              disabled={busy || Boolean(pendingImage)}
              onClick={() => void save()}
            >
              {busy ? "Saving…" : "Save profile"}
            </Button>
          </div>
        </section>
      )}
      <div className={`${styles.columns} ${profile.own ? "" : styles.viewerColumns}`}>
        <section className={styles.mainColumn}>
          <nav className={styles.tabs} aria-label="Profile sections">
            <Button appearance="standard"
              variant="ghost"
              type="button"
              aria-current={tab === "posts" ? "page" : undefined}
              onClick={() => setTab("posts")}
            >
              Posts
            </Button>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              aria-current={tab === "about" ? "page" : undefined}
              onClick={() => setTab("about")}
            >
              About
            </Button>
          </nav>
          {tab === "posts" ? (
            <div className={styles.posts}>
              {profile.posts.length ? (
                profile.posts.map((post) => (
                  <article className={styles.post} key={post.id}>
                    <div className={styles.postHead}>
                      <span className={styles.postAvatar}>
                        {picture ? (
                          <Image unoptimized src={picture} alt="" fill sizes="38px" />
                        ) : (
                          <UserRound size={20} />
                        )}
                      </span>
                      <div>
                        <strong>{profile.name}</strong>
                        <small>
                          <TimeText appearance="standard" value={post.created_at} /> · {post.audienceLabel}
                        </small>
                      </div>
                    </div>
                    <p>{post.body}</p>
                    <Link href={canonicalUiHref(`${base}/posts/${encodeURIComponent(post.id)}`)}>
                      Open conversation →
                    </Link>
                  </article>
                ))
              ) : (
                <div className={styles.empty}>
                  No posts are available in your current work scope.
                </div>
              )}
              {profile.nextBefore && (
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  className={styles.addLink}
                  disabled={moreBusy}
                  onClick={() => void loadMore()}
                >
                  {moreBusy ? "Loading…" : "Load older posts"}
                </Button>
              )}
            </div>
          ) : (
            <div className={styles.about}>
              <h2>About {profile.name}</h2>
              {profile.bio ? (
                <p>{profile.bio}</p>
              ) : (
                <p className={styles.empty}>No introduction yet.</p>
              )}
              {profile.work.map((item, index) => (
                <p key={index}>
                  <MapPin size={16} /> {item.title || "Team member"} · {item.company}
                </p>
              ))}
              {profile.links.map((link) => (
                <a key={link.url} href={link.url} target="_blank" rel="noopener noreferrer">
                  <ExternalLink size={15} /> {link.label}
                </a>
              ))}
            </div>
          )}
        </section>
        {profile.own && (
          <aside className={styles.sideColumn}>
            <TimePreferenceEditor />
          </aside>
        )}
      </div>
    </div>
  );
}
