"use client";
import { workFetch } from "@/lib/work-fetch";
import { useState } from "react";
import type { Mention } from "@/lib/social-text";
import { SocialTextInput } from "./social-text-input";
import { SocialTextBody } from "./social-text-body";
import type { HomePost } from "@/server/social-home";
import type { Meta } from "./social-types";
import { useSocialCommand } from "./social-command";
export function SocialComments({
  api,
  post,

  onChanged,
}: {
  api: string;
  post: HomePost;
  meta: Meta;
  userId: string;
  onChanged: () => Promise<void>;
}) {
  const [loaded,setLoaded] = useState<HomePost["comments"]>([]);
  const [cursor,setCursor] = useState<string|null|undefined>(undefined);
  const comments = [...new Map([...loaded,...post.comments].map(c=>[c.id,c])).values()].sort((a,b)=>a.created_at-b.created_at || a.id.localeCompare(b.id));
  const loadOlder = () => run(async()=>{
    const response=await workFetch(`${api}/posts/${post.id}/comments${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ''}`,{cache:"no-store"});
    const result=await response.json();
    if(!response.ok) throw new Error(result.error?.message??"Unable to load comments");
    setLoaded(old=>[...old,...result.data.items]);setCursor(result.data.nextCursor);
  });
  const [body,setBody]=useState("");
  const [mentions,setMentions]=useState<Mention[]>([]);
  const { run, send, busy, message } = useSocialCommand();
  return (
    <>
      <div className="beta-comments">
        {comments.map((comment) => (
          <div id={comment.id} className="beta-comment" key={comment.id}>
            <div>
              <strong>{comment.author_name}{comment.author_kind==="agent"?" · AI":""}</strong>
              <SocialTextBody body={comment.body} mentions={comment.mentions} base={api.replace(/^\/api\/v1/, "")}/>
            </div>
          </div>
        ))}
        {comments.length < post.commentCount && cursor !== null && <button disabled={busy} onClick={loadOlder}>Load older comments</button>}
        <form
          onSubmit={(e) => {
            e.preventDefault();

            run(async () => {
              await send(`${api}/posts/${post.id}`, {
                action: "comment",
                body, mentions,
              });
              setBody(""); setMentions([]);
              await onChanged();
            });
          }}
        >
          <SocialTextInput api={api} value={body} mentions={mentions} onChange={(body,mentions)=>{setBody(body);setMentions(mentions);}} maxLength={1000} label="Write a comment" placeholder="Add to the conversation…" disabled={busy}/>
          <button disabled={busy || !body.trim()}>Reply</button>
        </form>
      </div>
      <p role="status">{message}</p>
    </>
  );
}
