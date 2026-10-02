"use client";

import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import Image from "next/image";
import { Camera, ExternalLink, Link2, MapPin, Pencil, UserRound } from "lucide-react";
import { workFetch } from "@/lib/work-fetch";
import { TimeText } from "@/components/time/time-preference-context";
import { TimePreferenceEditor } from "@/components/time/time-preference-editor";
import type { memberProfileView } from "@/server/member-profiles";
import styles from "./profile-page.module.css";

type Profile = Awaited<ReturnType<typeof memberProfileView>>;

export function ProfilePage({ initial, base }: {
  initial: Profile; base: string;
}) {
  const [profile, setProfile] = useState(initial);
  const [tab, setTab] = useState<"posts"|"about">("posts");
  const [editing, setEditing] = useState(false);
  const [bio, setBio] = useState(initial.bio);
  const [links, setLinks] = useState(initial.links);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [avatarPreview, setAvatarPreview] = useState<string|null>(null);
  const [coverPreview, setCoverPreview] = useState<string|null>(null);
  const [pendingImage, setPendingImage] = useState<{kind: "avatar"|"cover"; file: File}|null>(null);
  const [moreBusy, setMoreBusy] = useState(false);
  const avatarInput = useRef<HTMLInputElement>(null);
  const coverInput = useRef<HTMLInputElement>(null);
  useEffect(() => () => { if (avatarPreview) URL.revokeObjectURL(avatarPreview); if (coverPreview) URL.revokeObjectURL(coverPreview); }, [avatarPreview,coverPreview]);
  const api = `/api/v1${base}/profiles`;
  async function save() {
    setBusy(true); setMessage("");
    try {
      const response = await workFetch(`${api}/me`, {
        method: "PATCH", headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify({ bio, links, version: profile.version }),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to save profile.");
      setProfile(current => ({ ...current, bio: body.data.bio, links: body.data.links, version: body.data.version }));
      setEditing(false); setMessage("Profile saved.");
    } catch (error) { setMessage(error instanceof Error ? error.message : "Unable to save profile."); }
    finally { setBusy(false); }
  }
  function stageImage(kind: "avatar"|"cover", file: File) {
    const max = kind === "avatar" ? 2 * 1024 * 1024 : 5 * 1024 * 1024;
    if (!["image/jpeg","image/png","image/webp"].includes(file.type) || file.size > max) {
      setMessage(`Choose a JPEG, PNG or WebP ${kind} under ${kind === "avatar" ? 2 : 5} MB.`); return;
    }
    const preview = URL.createObjectURL(file);
    setAvatarPreview(kind === "avatar" ? preview : null);
    setCoverPreview(kind === "cover" ? preview : null);
    setPendingImage({kind,file}); setEditing(true); setMessage("");
  }
  function discardImage() {
    setPendingImage(null); setAvatarPreview(null); setCoverPreview(null);
  }
  async function upload(kind: "avatar"|"cover", file: File) {
    setBusy(true); setMessage("");
    try {
      const response = await workFetch(`${api}/me/media/${kind}`, {
        method: "POST", headers: { "Content-Type": file.type, "x-profile-version": String(profile.version), "Idempotency-Key": crypto.randomUUID() }, body: file,
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to upload image.");
      setProfile(current => ({ ...current, version: body.data.version,
        [kind === "avatar" ? "avatarUrl" : "coverUrl"]: `${api}/${encodeURIComponent(current.userId)}/media/${kind}?v=${body.data.version}` }));
      discardImage();
      setMessage(`${kind === "avatar" ? "Avatar" : "Cover"} updated.`);
    } catch (error) { setMessage(error instanceof Error ? error.message : "Unable to upload image."); }
    finally { setBusy(false); }
  }
  async function removeImage(kind: "avatar"|"cover") {
    setBusy(true); setMessage("");
    try {
      const response = await workFetch(`${api}/me/media/${kind}`, {
        method: "DELETE", headers: { "x-profile-version": String(profile.version), "Idempotency-Key": crypto.randomUUID() },
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to remove image.");
      setProfile(current => ({ ...current, version: body.data.version,
        [kind === "avatar" ? "avatarUrl" : "coverUrl"]: null }));
      setMessage(`${kind === "avatar" ? "Avatar" : "Cover"} removed.`);
    } catch (error) { setMessage(error instanceof Error ? error.message : "Unable to remove image."); }
    finally { setBusy(false); }
  }
  async function loadMore() {
    if (!profile.nextBefore || moreBusy) return;
    setMoreBusy(true);
    try {
      const response = await workFetch(`${api}/${encodeURIComponent(profile.userId)}?before=${profile.nextBefore}&beforeId=${encodeURIComponent(profile.nextBeforeId ?? "")}`, { cache: "no-store" });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error?.message ?? "Unable to load posts.");
      setProfile(current => ({ ...current, posts: [...current.posts, ...body.data.posts.filter((post: Profile["posts"][number]) => !current.posts.some(existing => existing.id === post.id))], nextBefore: body.data.nextBefore, nextBeforeId: body.data.nextBeforeId }));
    } catch (error) { setMessage(error instanceof Error ? error.message : "Unable to load posts."); }
    finally { setMoreBusy(false); }
  }
  const picture = avatarPreview ?? profile.avatarUrl;
  const banner = coverPreview ?? profile.coverUrl;
  return <main className={styles.page}>
    <Link className={styles.back} href={`${base}/people`}>← People</Link>
    <section className={styles.hero}>
      <div className={styles.cover}>
        {banner && <Image unoptimized src={banner} alt={`${profile.name}'s cover`} fill sizes="(max-width: 800px) 100vw, 1100px" className={styles.coverImage} />}
        {profile.own && <><button type="button" className={styles.coverEdit} disabled={busy} onClick={() => coverInput.current?.click()}><Camera size={16} /> Change cover</button><input ref={coverInput} className={styles.hiddenFile} aria-label="Cover image file" tabIndex={-1} type="file" accept="image/jpeg,image/png,image/webp" disabled={busy} onChange={event => { const file=event.target.files?.[0]; if(file) stageImage("cover",file); event.target.value=""; }} /></>}
      </div>
      <div className={styles.identity}>
        <div className={styles.avatar}>
          {picture ? <Image unoptimized src={picture} alt={`${profile.name}'s avatar`} fill sizes="112px" /> : <UserRound size={50} aria-hidden="true" />}
          {profile.own && <><button type="button" className={styles.avatarEdit} aria-label="Change avatar" disabled={busy} onClick={() => avatarInput.current?.click()}><Camera size={16} /></button><input ref={avatarInput} className={styles.hiddenFile} aria-label="Avatar image file" tabIndex={-1} type="file" accept="image/jpeg,image/png,image/webp" disabled={busy} onChange={event => { const file=event.target.files?.[0]; if(file) stageImage("avatar",file); event.target.value=""; }} /></>}
        </div>
        <div className={styles.identityText}><span className={styles.eyebrow}>Member profile</span><h1>{profile.name}</h1>
          {profile.work.length > 0 && <p className={styles.role}>{profile.work[0].title || "Team member"} · {profile.work[0].company}</p>}
          {profile.bio && <p className={styles.bio}>{profile.bio}</p>}
          {profile.links.length > 0 && <div className={styles.links}>{profile.links.map(link => <a key={link.url} href={link.url} target="_blank" rel="noopener noreferrer"><Link2 size={14} /> {link.label} <ExternalLink size={12} /></a>)}</div>}
        </div>
        {profile.own && <button type="button" className={styles.editButton} onClick={() => { setBio(profile.bio); setLinks(profile.links); if (editing) discardImage(); setEditing(value => !value); }}><Pencil size={16} /> {editing ? "Close editor" : "Edit profile"}</button>}
      </div>
    </section>
    {message && <p className={styles.message} role="status">{message}</p>}
    {editing && profile.own && <section className={styles.editor} aria-label="Edit your profile"><h2>Make it yours</h2>{pendingImage && <div className={`${styles.imageActions} ${styles.imagePreview}`} role="status"><p>Previewing {pendingImage.kind}. Review the image above before saving.</p><button type="button" className="button" disabled={busy} onClick={() => void upload(pendingImage.kind,pendingImage.file)}>{busy ? "Saving…" : `Save ${pendingImage.kind}`}</button><button type="button" disabled={busy} onClick={discardImage}>Cancel image</button></div>}<div className={styles.imageActions}>{profile.avatarUrl && <button type="button" disabled={busy} onClick={() => void removeImage("avatar")}>Remove avatar</button>}{profile.coverUrl && <button type="button" disabled={busy} onClick={() => void removeImage("cover")}>Remove cover</button>}</div><label>About you<textarea maxLength={500} rows={4} value={bio} onChange={event => setBio(event.target.value)} placeholder="Tell your colleagues what you work on…" /><small>{bio.length}/500</small></label><h3>Links</h3>{links.map((link,index) => <div className={styles.linkRow} key={index}><input aria-label={`Link ${index+1} label`} maxLength={40} value={link.label} placeholder="Label" onChange={event => setLinks(current => current.map((item,i) => i===index ? {...item,label:event.target.value} : item))} /><input aria-label={`Link ${index+1} URL`} type="url" value={link.url} placeholder="https://" onChange={event => setLinks(current => current.map((item,i) => i===index ? {...item,url:event.target.value} : item))} /><button type="button" onClick={() => setLinks(current => current.filter((_,i) => i!==index))}>Remove</button></div>)}{links.length < 3 && <button type="button" className={styles.addLink} onClick={() => setLinks(current => [...current,{label:"",url:""}])}>+ Add a link</button>}<div><button className="button" type="button" disabled={busy || Boolean(pendingImage)} onClick={() => void save()}>{busy ? "Saving…" : "Save profile"}</button></div></section>}
    <div className={`${styles.columns} ${profile.own ? "" : styles.viewerColumns}`}>
      <section className={styles.mainColumn}><nav className={styles.tabs} aria-label="Profile sections"><button type="button" aria-current={tab==="posts"?"page":undefined} onClick={() => setTab("posts")}>Posts</button><button type="button" aria-current={tab==="about"?"page":undefined} onClick={() => setTab("about")}>About</button></nav>
        {tab === "posts" ? <div className={styles.posts}>{profile.posts.length ? profile.posts.map(post => <article className={styles.post} key={post.id}><div className={styles.postHead}><span className={styles.postAvatar}>{picture ? <Image unoptimized src={picture} alt="" fill sizes="38px" /> : <UserRound size={20} />}</span><div><strong>{profile.name}</strong><small><TimeText value={post.created_at} /> · {post.audienceLabel}</small></div></div><p>{post.body}</p><Link href={`${base}/posts/${encodeURIComponent(post.id)}`}>Open conversation →</Link></article>) : <div className={styles.empty}>No posts are available in your current work scope.</div>}{profile.nextBefore && <button type="button" className={styles.addLink} disabled={moreBusy} onClick={() => void loadMore()}>{moreBusy ? "Loading…" : "Load older posts"}</button>}</div>
          : <div className={styles.about}><h2>About {profile.name}</h2>{profile.bio ? <p>{profile.bio}</p> : <p className={styles.empty}>No introduction yet.</p>}{profile.work.map((item,index) => <p key={index}><MapPin size={16} /> {item.title || "Team member"} · {item.company}</p>)}{profile.links.map(link => <a key={link.url} href={link.url} target="_blank" rel="noopener noreferrer"><ExternalLink size={15} /> {link.label}</a>)}</div>}
      </section>
      {profile.own && <aside className={styles.sideColumn}><TimePreferenceEditor /></aside>}
    </div>
  </main>;
}
