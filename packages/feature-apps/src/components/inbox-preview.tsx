"use client";

import { useState } from "react";
import { ArrowLeft, Bell, CheckCheck, ChevronRight, Hash, MessageCircle, Pencil, Search, Send, Users, X } from "lucide-react";
import { Dialog } from "@erpx/ui/components/dialog";
import styles from "./inbox-preview.module.css";

type View = "messages" | "teams" | "updates";
type Conversation = {
  id: string;
  name: string;
  initials: string;
  detail: string;
  preview: string;
  time: string;
  unread: number;
  color: "blue" | "purple" | "green";
  messages: { text: string; time: string; mine?: boolean }[];
};

// Design samples only. This screen does not read or write messaging data.
const conversations: Conversation[] = [
  { id: "alex", name: "Alex Morgan", initials: "AM", detail: "Direct message", preview: "The updated brief is ready for your review.", time: "9:41 AM", unread: 2, color: "blue", messages: [
    { text: "Hi! I’ve pulled together the notes from our planning session.", time: "9:38 AM" },
    { text: "Thanks, Alex. Could you include the next steps as well?", time: "9:39 AM", mine: true },
    { text: "Of course. The updated brief is ready for your review.", time: "9:41 AM" },
  ] },
  { id: "design", name: "Design catch-up", initials: "DC", detail: "Group conversation · 4 members", preview: "Taylor: These are looking great. Thank you!", time: "9:15 AM", unread: 1, color: "purple", messages: [
    { text: "Here’s a space to share ideas before our next design catch-up.", time: "9:10 AM" },
    { text: "I’ve added a few options for us to walk through together.", time: "9:12 AM", mine: true },
    { text: "These are looking great. Thank you!", time: "9:15 AM" },
  ] },
  { id: "sam", name: "Sam Rivera", initials: "SR", detail: "Direct message", preview: "Sounds good, see you then.", time: "Yesterday", unread: 0, color: "green", messages: [
    { text: "Shall we catch up tomorrow morning?", time: "4:20 PM" },
    { text: "Yes, 10 works for me.", time: "4:22 PM", mine: true },
    { text: "Sounds good, see you then.", time: "4:23 PM" },
  ] },
];

const channels: Conversation[] = [
  { id: "general", name: "general", initials: "#", detail: "Everyone · General discussion", preview: "A shared space to keep everyone in the loop.", time: "10:00 AM", unread: 1, color: "blue", messages: [
    { text: "Welcome to general. Share updates, ask a question, or celebrate a small win with the team.", time: "10:00 AM" },
  ] },
  { id: "ideas", name: "ideas-and-feedback", initials: "#", detail: "Everyone · Ideas & feedback", preview: "Every idea starts a conversation.", time: "Yesterday", unread: 0, color: "purple", messages: [
    { text: "Have an idea that could make our day a little easier? This is the place to share it.", time: "2:30 PM" },
  ] },
];

const views = [
  { id: "messages", label: "Messages", icon: MessageCircle },
  { id: "teams", label: "Teams", icon: Users },
  { id: "updates", label: "Updates", icon: Bell },
] as const;

export function InboxPreview() {
  const [view, setView] = useState<View>("messages");
  const [query, setQuery] = useState("");
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [composing, setComposing] = useState(false);
  const [draft, setDraft] = useState("");
  const [recipient, setRecipient] = useState("");
  const [newDraft, setNewDraft] = useState("");
  const source = view === "teams" ? channels : conversations;
  const rooms = source.filter(room => (!unreadOnly || room.unread > 0) && `${room.name} ${room.preview}`.toLowerCase().includes(query.trim().toLowerCase()));
  const room = source.find(item => item.id === selected);

  function changeView(next: View) {
    setView(next);
    setSelected(null);
    setQuery("");
    setUnreadOnly(false);
    setDraft("");
  }

  return <div className={styles.page} data-appearance="standard" data-source="preview">
    <header className={styles.heading}>
      <div><p className={styles.eyebrow}>YOUR WORKSPACE</p><h1>Inbox</h1><p className={styles.subtitle}>Conversations and team updates, all in one place.</p></div>
      <button type="button" className={styles.primary} onClick={() => setComposing(true)}><Pencil size={16} aria-hidden="true" />New message</button>
    </header>

    <div className={styles.previewNote}><span className={styles.previewBadge}>UI preview</span><span>Sample conversations. Messages won’t be sent.</span></div>

    <section className={styles.workspace} aria-label="Inbox preview">
      <nav className={styles.tabs} aria-label="Inbox views">
        {views.map(({ id, label, icon: Icon }) => <button type="button" key={id} aria-pressed={view === id} onClick={() => changeView(id)}><Icon size={17} aria-hidden="true" />{label}{id === "messages" && <span className={styles.tabCount}>3</span>}</button>)}
      </nav>

      {view === "updates" ? <div className={styles.updates}>
        <span className={styles.emptyIcon}><CheckCheck size={30} aria-hidden="true" /></span><h2>You’re all caught up</h2><p>Notifications and mentions will have a home here.</p><span className={styles.sampleLabel}>No updates in this preview</span>
      </div> : <div className={`${styles.panes} ${room ? styles.hasSelection : ""}`}>
        <aside className={styles.list} aria-label={view === "teams" ? "Team channels" : "Conversations"}>
          <div className={styles.listTools}>
            <label className={styles.search}><Search size={17} aria-hidden="true" /><input aria-label={view === "teams" ? "Search channels" : "Search conversations"} placeholder={view === "teams" ? "Search channels" : "Search messages"} value={query} onChange={event => setQuery(event.target.value)} />{query && <button type="button" aria-label="Clear search" onClick={() => setQuery("")}><X size={14} /></button>}</label>
            <div className={styles.listHeading}><h2>{view === "teams" ? "Everyone" : "Conversations"}</h2><button type="button" className={styles.filter} aria-pressed={unreadOnly} onClick={() => setUnreadOnly(!unreadOnly)}>Unread</button></div>
          </div>
          <div className={styles.rooms}>
            {rooms.map(item => <button type="button" key={item.id} className={styles.room} aria-pressed={selected === item.id} onClick={() => { setSelected(item.id); setDraft(""); }}>
              <span className={`${styles.avatar} ${styles[item.color]}`}>{view === "teams" ? <Hash size={21} aria-hidden="true" /> : item.initials}</span>
              <span className={styles.roomCopy}><span className={styles.roomTop}><strong>{item.name}</strong><small>{item.time}</small></span><span className={styles.roomBottom}><span>{item.preview}</span>{item.unread > 0 && <span className={styles.unread} aria-label={`${item.unread} unread sample messages`}>{item.unread}</span>}</span></span>
            </button>)}
            {!rooms.length && <div className={styles.noResults} role="status"><Search size={24} aria-hidden="true" /><strong>No conversations found</strong><p>Try another search or turn off the unread filter.</p><button type="button" className={styles.textButton} onClick={() => { setQuery(""); setUnreadOnly(false); }}>Clear filters</button></div>}
          </div>
          <p className={styles.listFooter}>SAMPLE WORKSPACE</p>
        </aside>

        {room ? <section className={styles.conversation} aria-label={`Conversation with ${room.name}`}>
          <header className={styles.conversationHeader}><button type="button" className={styles.back} aria-label="Back to conversations" onClick={() => setSelected(null)}><ArrowLeft size={20} /></button><span className={`${styles.avatar} ${styles[room.color]}`}>{room.initials}</span><div><h2>{room.name}</h2><p>{room.detail}</p></div><span className={styles.sampleLabel}>Sample</span></header>
          <div className={styles.history}><div className={styles.dateDivider}>Sample conversation</div>{room.messages.map((message, index) => <div key={index} className={`${styles.message} ${message.mine ? styles.mine : ""}`}><span>{message.mine ? "You" : room.name}</span><p>{message.text}</p><small>{message.time}{message.mine && <CheckCheck size={14} aria-label="Sample read receipt" />}</small></div>)}</div>
          <div className={styles.composer}><label className={styles.srOnly} htmlFor="inbox-draft">Message draft</label><textarea id="inbox-draft" placeholder="Write a message…" value={draft} onChange={event => setDraft(event.target.value)} rows={2} /><div><span>Preview only · sending is unavailable</span><button type="button" disabled aria-label="Send message unavailable in UI preview"><Send size={17} aria-hidden="true" /></button></div></div>
        </section> : <section className={styles.welcome} aria-label="Choose a conversation">
          <div className={styles.welcomeArtwork} aria-hidden="true"><span className={styles.artBubble}><MessageCircle size={37} strokeWidth={1.4} /></span><span className={styles.artSmall}><Users size={19} /></span></div>
          <span className={styles.eyebrow}>A SPACE TO CONNECT</span><h2>{view === "teams" ? "Better together" : "Good conversations move work forward"}</h2><p>{view === "teams" ? "Bring the whole team into the conversation. Choose a channel to take a look around." : "A quick question, a new idea, or a little progress to share. Choose a conversation to get started."}</p>
          <button type="button" className={styles.primary} onClick={() => view === "teams" ? setSelected("general") : setComposing(true)}>{view === "teams" ? "Explore a channel" : "Start a conversation"}<ChevronRight size={16} aria-hidden="true" /></button>
          <span className={styles.welcomeHint}>Your conversations, thoughtfully connected.</span>
        </section>}
      </div>}
    </section>

    <Dialog appearance="standard" presentation="form-drawer" open={composing} onClose={() => setComposing(false)} title="New message">
      <div className={styles.drawer}><p className={styles.drawerIntro}>Start with a person and a thought worth sharing.</p><div className={styles.previewNote}><span className={styles.previewBadge}>UI preview</span><span>This draft stays on this page and won’t be sent.</span></div><label>To<input placeholder="Enter a name" value={recipient} onChange={event => setRecipient(event.target.value)} /></label><label>Message<textarea rows={7} placeholder="What would you like to share?" value={newDraft} onChange={event => setNewDraft(event.target.value)} /></label><div className={styles.drawerActions}><button type="button" className={styles.secondary} onClick={() => setComposing(false)}>Close preview</button><button type="button" className={styles.primary} disabled><Send size={16} aria-hidden="true" />Send message</button></div><p className={styles.drawerHint}>Sending is unavailable in this UI preview.</p></div>
    </Dialog>
  </div>;
}
