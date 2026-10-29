'use client';
import {holdingHostForHost} from '@erpx/feature-runtime/lib/holding-hosts';
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import { verifiedFormSubmissions } from "@erpx/feature-identity_access/lib/verified-form-submit";
import { createContext, useContext, useEffect, useId, useRef, useState, type ReactNode, type FormEvent } from 'react';
import { Check, ChevronDown, LogIn, Plus, ShieldCheck, UserRound, X } from 'lucide-react';
import { Button, Field, TextInput, Select, Popover, PopoverTrigger, PopoverContent } from '@erpx/ui/components/foundation';
import { Dialog } from '@erpx/ui/components/dialog';
import { InteractionSuspension } from '@erpx/ui/components/interaction-suspension';
import { PersonAvatar } from '@erpx/ui/components/person-avatar';
import {Appearance} from '@erpx/workplace/components/workplace-controls';
import { useHydrated } from '@erpx/ui/components/use-hydrated';
import { useAccountPreferences } from '@erpx/feature-identity_access/components/account-preferences-context';
import { confirmContextSwitch, contextSwitchBlockReason, hasUnsavedWork, setContextSwitching } from '@erpx/feature-work_projection/lib/context-switch';
import type { DeviceAccounts, SwitcherContext, SwitcherRole } from '@erpx/feature-identity_access/lib/device-accounts';
import {deviceDirectoryLogin} from '@erpx/server-identity_access/public/rules/device-account-return';
import styles from '@erpx/workplace/components/product-account.module.css';

type User = { id: string; name: string; email: string };
type Placement = { roleLabel?: string; companyId?: string; workspaceBase?: string; sidebar?: boolean; language?:'th'|'en' };
type Menu = {
  user: User; th: boolean; busy: boolean; owner: string | null; error: string; accounts: DeviceAccounts | null;
  roles: SwitcherContext | null; loading: boolean; workspace: string; setWorkspace(key: string): void;
  accountOrigin:string; appsHref:string;
  open(id: string, placement: Placement): void; close(): void; add(): void;
  choose(handle: string): void; remove(handle: string): void; chooseRole(role: SwitcherRole, provider?: boolean): void;
};
const Context = createContext<Menu | null>(null);
export const useProductAccount = () => useContext(Context);
async function read<T>(response: Response): Promise<T> {
  const body = await response.json().catch(() => null);
  if (!response.ok || !body?.data) throw Object.assign(new Error(body?.error?.message ?? 'Could not verify the account response. Try again.'), { status: response.status,code:body?.error?.code });
  return body.data as T;
}

export function ProductAccountProvider({ user, audience, accountOrigin, appsHref, children }: {
  user: User; audience: string; accountOrigin: string; appsHref:string; children: ReactNode;
}) {
  const original = useRef(user), keptChildren = useRef<ReactNode>(null), mounted = useRef(true);
  const preferences = useAccountPreferences(),[menuLanguage,setMenuLanguage]=useState<'th'|'en'|null>(null);
  const th = (menuLanguage ?? preferences?.preference?.language) === 'th';
  const [owner, setOwner] = useState<string | null>(null), [accounts, setAccounts] = useState<DeviceAccounts | null>(null);
  const [roles, setRoles] = useState<SwitcherContext | null>(null), [workspace, setWorkspace] = useState('');
  const [busy, setBusy] = useState(false), [loading, setLoading] = useState(false), [adding, setAdding] = useState(false);
  const [error, setError] = useState(''), [stale, setStale] = useState<'changed'|'unavailable'|null>(null);
  const [unverified,setUnverified]=useState(false),[checkingOutcome,setCheckingOutcome]=useState(false);
  const [returnHandle, setReturnHandle] = useState<string | null>(null);
  const [resumeUrl,setResumeUrl]=useState<string|null>(null),resumeFrame=useRef<HTMLIFrameElement>(null),staleNotice=useRef(false),returning=useRef(false);
  const lock = useRef(false), roleTicket = useRef(0), checkFlight = useRef<Promise<boolean>|null>(null), verifiedSubmissions = useRef(verifiedFormSubmissions<HTMLFormElement, Event>());
  const lastPlacement = useRef<Placement>({}), closeFocus = useRef<HTMLElement | null>(null), formRef = useRef<HTMLFormElement>(null);
  const draftForms=useRef(new WeakSet<HTMLFormElement>());
  const t = (thai: string, english: string) => th ? thai : english;
  function errorMessage(cause: unknown, fallback: string) {
    const translated: Record<string,string> = {
      SIGN_IN_FAILED: 'ลงชื่อเข้าใช้ไม่สำเร็จ ตรวจสอบอีเมล รหัสผ่าน หรือใช้ Directory ที่บัญชีได้รับอนุญาต',
      DEVICE_ACCOUNT_LIMIT: 'บันทึกได้สูงสุด 5 บัญชี กรุณานำบัญชีเดิมออกจากอุปกรณ์นี้ก่อน',
      DEVICE_ACCOUNTS_CHANGED: 'รายการบัญชีเปลี่ยนแล้ว กรุณาตรวจรายการปัจจุบันก่อนลองอีกครั้ง',
      DEVICE_ACCOUNT_EXPIRED: 'บัญชีนี้หมดอายุแล้ว กรุณาลงชื่อเข้าใช้อีกครั้ง',
      ACCOUNT_SIGN_IN_PENDING: 'มีการลงชื่อเข้าใช้อยู่ กรุณาตรวจผลก่อนลองอีกครั้ง',
      DEVICE_ACCOUNT_REENTRY_REQUIRED: 'ระงับงานของบัญชีเดิมไม่สำเร็จ กรุณาเข้าใช้งานใหม่ผ่าน Account',
    };
    return th && translated[(cause as {code?:string})?.code ?? ''] || (cause instanceof Error ? cause.message : fallback);
  }
  // Render the current RSC slot directly. Only retain a committed tree for
  // identity recovery; a ref captured during render may hold an unfinished
  // streamed Suspense slot across React's server/client render attempts.
  useEffect(()=>{if(user.id===original.current.id)keptChildren.current=children;},[children,user.id]);
  const api = (action = '') => accountOrigin + '/api/v1/account/device-accounts' + action;
  async function accountRead() {
    let next = await read<DeviceAccounts>(await holdingFetch(api(), { credentials: 'include', cache: 'no-store' }));
    if (!next.initialized) next = await read<DeviceAccounts>(await holdingFetch(api('/initialize'), { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: '{}' }));
    if (mounted.current) {
      setAccounts(next);
      const current = next.accounts.find(account => account.current);
      const own = next.accounts.find(account => account.email.toLowerCase() === original.current.email.toLowerCase());
      if (own) setReturnHandle(own.handle);
      if (current && current.email.toLowerCase() !== original.current.email.toLowerCase()&&!staleNotice.current) changed();
    }
    return next;
  }
  function changed() {
    setStale('changed');
    // Repeated focus/poll failures must not close the recovery drawer or
    // reveal an old draft while the user is reauthenticating.
    if(staleNotice.current)return;
    staleNotice.current=true;setOwner(null); setAdding(false); formRef.current?.reset(); setContextSwitching(true);
    void accountRead().catch(()=>{});
  }
  function check():Promise<boolean> {
    if(checkFlight.current)return checkFlight.current;
    if(lock.current||document.visibilityState!=='visible')return Promise.resolve(false);
    const pending=checkAccount();checkFlight.current=pending;void pending.finally(()=>{if(checkFlight.current===pending)checkFlight.current=null;});return pending;
  }
  async function checkAccount() {
    try {
      const response = await holdingFetch('/api/v1/account/switcher-context?check=1', { cache: 'no-store' });
      if (!mounted.current) return false;
      if (response.status === 401) { changed(); return false; }
      const next = await read<{ user: User }>(response);
      if (next.user.id !== original.current.id) { changed(); return false; }
      if(staleNotice.current)return false;
      setStale(null); return true;
    } catch { if (mounted.current) setStale(staleNotice.current?'changed':'unavailable'); return false; }
  }
  useEffect(() => {
    mounted.current = true;
    document.documentElement.dataset.deviceAccounts = 'true';
    const focus = () => { void check(); }, expired = () => changed();
    const restored = (event: PageTransitionEvent) => { if (event.persisted) focus(); };
    window.addEventListener('focus', focus); window.addEventListener('pageshow', restored);
    window.addEventListener('erpx:device-account-stale', expired);
    const resumed=(event:MessageEvent)=>{
      if(event.origin!==window.location.origin||event.source!==resumeFrame.current?.contentWindow||event.data?.type!=='erpx:account-resumed')return;
      setResumeUrl(null);returning.current=false;
      if(event.data.ok&&event.data.userId===original.current.id){staleNotice.current=false;setStale(null);setContextSwitching(false);setError('');window.dispatchEvent(new Event('erpx:work-context-changed'));void accountRead().catch(()=>{});}
      else setError('The original account could not reopen this workspace. Review its current access.');
    };
    window.addEventListener('message',resumed);
    const saved=(event:Event)=>{
      const detail=(event as CustomEvent<{form?:HTMLFormElement;formEditVersion?:string|null}>).detail;
      if(detail?.form&&draftForms.current.has(detail.form)&&detail.formEditVersion&&detail.formEditVersion===detail.form.getAttribute('data-erpx-edit-version'))detail.form.removeAttribute('data-work-dirty');
    };
    window.addEventListener('erpx:record-command-saved',saved);
    document.addEventListener('visibilitychange', focus);
    const timer = window.setInterval(focus, 30000);
    if (audience === 'apps') void roleRead('');
    const invalidateRoleRequests = () => { ++roleTicket.current; };
    return () => {
      mounted.current = false; invalidateRoleRequests(); clearInterval(timer);
      window.removeEventListener('focus', focus); window.removeEventListener('pageshow', restored);
      window.removeEventListener('erpx:device-account-stale', expired); document.removeEventListener('visibilitychange', focus);
      window.removeEventListener('message',resumed);
      window.removeEventListener('erpx:record-command-saved',saved);
      delete document.documentElement.dataset.deviceAccounts;
    };
  // The mounted account owns the guarded draft until a full navigation.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // The first mounted account owns the draft; prop changes only suspend it.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { if (user.id !== original.current.id) changed(); }, [user.id]);
  async function roleRead(key: string, placement?: Placement) {
    const ticket = ++roleTicket.current; setLoading(true);
    try {
      const next = await read<SwitcherContext>(await holdingFetch('/api/v1/account/switcher-context' + (key ? '?' + new URLSearchParams({ workspace: key }) : ''), { cache: 'no-store' }));
      if (!mounted.current || ticket !== roleTicket.current) return;
      if (next.user.id !== original.current.id) { changed(); return; }
      setRoles(next); setWorkspace(key);
      if (!key && placement?.companyId) {
        const scope = next.workspaces.find(choice => choice.productId === audience && choice.companyId === placement.companyId && (placement.workspaceBase ? choice.target.startsWith(placement.workspaceBase + '/') : window.location.pathname.startsWith(choice.target)));
        if (scope) { await roleRead(scope.key); return; }
      }
      if (!key && audience === 'apps') {
        let remembered: string | null = null;
        try { remembered = sessionStorage.getItem('erpx:account-workspace:' + original.current.id); } catch { /* Storage is optional. */ }
        // A remembered workspace is only a hint; the owner-filtered choices
        // must still contain it before any assigned role can be restored.
        if (remembered && next.workspaces.some(choice => choice.key === remembered)) { await roleRead(remembered); return; }
      }
    } catch (cause) { if (mounted.current && ticket === roleTicket.current) setError(errorMessage(cause, 'Could not read roles.')); }
    finally { if (ticket === roleTicket.current) setLoading(false); }
  }
  function guard() {
    const blocked = contextSwitchBlockReason();
    if (blocked) { setError(blocked); return false; }
    return confirmContextSwitch(t('เปลี่ยนบัญชีหรือบทบาทและทิ้งข้อมูลที่ยังไม่บันทึกหรือไม่?', 'Change account or role and discard unsaved changes?'));
  }
  function continueHere() {
    const companyId=lastPlacement.current.companyId??document.querySelector<HTMLElement>('[data-work-company]')?.dataset.workCompany;
    const next = '/account/continue?' + new URLSearchParams({ app:audience, next: window.location.pathname + window.location.search,
      ...(companyId ? { companyId } : {}) });
    window.location.assign(new URL('/sso/start?' + new URLSearchParams({ next }),window.location.origin).href);
  }
  async function operation(action: string, body: Record<string, unknown>, method = 'POST') {
    if (lock.current || unverified || !returning.current&&!guard()) return;
    lock.current = true; setBusy(true); setError(''); setContextSwitching(true);
    try {
      const next = await read<DeviceAccounts>(await holdingFetch(api(action), { method, credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }));
      if (!mounted.current) return;
      setAccounts(next); setOwner(null); formRef.current?.reset(); setAdding(false);
      const current = next.accounts.find(account => account.current);
      if (!current) { setStale('changed'); setContextSwitching(false); }
      else if (action === '' && current.email.toLowerCase() === original.current.email.toLowerCase()) setContextSwitching(false);
      else if(returning.current){
        const companyId=lastPlacement.current.companyId??document.querySelector<HTMLElement>('[data-work-company]')?.dataset.workCompany;
        const next='/account/continue?'+new URLSearchParams({app:audience,resume:'1',account:original.current.id,next:window.location.pathname+window.location.search,...(companyId?{companyId}:{})});
        setResumeUrl('/sso/start?'+new URLSearchParams({next}));
      }else continueHere();
    } catch (cause) {
      setContextSwitching(false);
      returning.current=false;
      if((cause as {code?:string})?.code==='DEVICE_ACCOUNT_REENTRY_REQUIRED')changed();
      setError(errorMessage(cause, 'Could not change account.'));
      formRef.current?.querySelector<HTMLInputElement>('input[name="password"]')?.setAttribute('value', '');
      const password = formRef.current?.querySelector<HTMLInputElement>('input[name="password"]'); if (password) password.value = '';
      // Read the actual outcome; never blindly replay a possibly committed login.
      try { await accountRead(); await checkAccount(); } catch {
        setOwner(null);setAdding(false);setUnverified(true);
        setError(t('ตรวจผลการเปลี่ยนบัญชีไม่ได้ กรุณาลองตรวจอีกครั้ง', 'Could not verify the account change. Check again before retrying.'));
      }
    } finally { lock.current = false; if (mounted.current) setBusy(false); }
  }
  async function chooseRole(role: SwitcherRole, provider = false) {
    if (!roles || lock.current || !guard()) return;
    lock.current = true; setBusy(true); setError(''); setContextSwitching(true);
    try {
      const next = await read<{ href: string }>(await holdingFetch('/api/v1/account/role-selection', { method: 'POST', headers: { 'Content-Type': 'application/json', 'Idempotency-Key': crypto.randomUUID() },
        body: JSON.stringify({ workspaceKey: provider ? 'provider' : workspace, assignmentId: role.id, version: role.version, revision: provider ? roles.provider!.revision : roles.revision, accountKey: original.current.id }) }));
      window.location.assign(next.href);
    } catch (cause) { setContextSwitching(false); setError(errorMessage(cause, 'Could not select role.')); await roleRead(workspace); }
    finally { lock.current = false; if (mounted.current) setBusy(false); }
  }
  async function submit(event: FormEvent<HTMLDivElement>) {
    if ((event.target as HTMLElement).closest('[data-account-switcher]')) return;
    const form = event.target as HTMLFormElement;
    // A portal can deliver the same native submit through more than one React
    // capture path. Keep its verified replay valid for that entire event.
    if (verifiedSubmissions.current.accepts(form, event.nativeEvent)) return;
    event.preventDefault(); event.stopPropagation();
    const submitter = (event.nativeEvent as SubmitEvent).submitter as HTMLButtonElement | null;
    if (await check()) verifiedSubmissions.current.replay(form, () => form.requestSubmit(submitter));
  }
  function edited(event:FormEvent<HTMLDivElement>){
    const target=event.target as HTMLElement;
    if(target.closest('[data-account-switcher], [data-work-dirty="false"]')||target.matches('input[type="search"], [role="combobox"]'))return;
    const form=target.closest('form');if(!form)return;
    // React capture includes portaled editors; native ancestor listeners do
    // not. Include those drafts in the existing explicit-discard guard.
    draftForms.current.add(form);form.dataset.workDirty='true';form.setAttribute('data-erpx-edit-version',crypto.randomUUID());
  }
  const value: Menu = {
    user: original.current, th, busy, owner, accounts, roles, loading, error, workspace, accountOrigin,appsHref,
    setWorkspace: key => {
      setError('');
      if (audience === 'apps') { try { if(key)sessionStorage.setItem('erpx:account-workspace:' + original.current.id,key);else sessionStorage.removeItem('erpx:account-workspace:' + original.current.id); } catch { /* Storage is optional. */ } }
      void roleRead(key);
    },
    open: (id, placement) => {
      setMenuLanguage(placement.language ?? null);
      lastPlacement.current = placement; closeFocus.current = document.activeElement as HTMLElement; setOwner(id); setError('');
      void accountRead().catch(cause => setError(errorMessage(cause, 'Could not read saved accounts.')));
      void roleRead('', placement);
    },
    close: () => setOwner(null), add: () => { setOwner(null); setError(''); setAdding(true); },
    choose: handle => { if (accounts) void operation('/active', { handle, revision: accounts.revision }); },
    remove: handle => { if (accounts) void operation('', { handle, revision: accounts.revision }, 'DELETE'); }, chooseRole,
  };
  return <Context value={value}>
    <InteractionSuspension value={busy || stale === 'changed' || unverified}><div inert={busy || stale === 'changed' || unverified} style={busy || stale === 'changed' || unverified ? { visibility: 'hidden' } : undefined} onInputCapture={edited} onSubmitCapture={event => { void submit(event); }}>{user.id===original.current.id?children:keptChildren.current}</div></InteractionSuspension>
    {busy && <div className={styles.veil} role="status">{t('กำลังตรวจบัญชีและสิทธิ์…', 'Updating account and permissions…')}</div>}
    <Dialog appearance="standard" presentation="form-drawer" open={adding} dismissDisabled={busy} onClose={() => { formRef.current?.reset(); setAdding(false); }} title={t('เพิ่มบัญชี', 'Add account')} finalFocus={() => closeFocus.current}>
      <form ref={formRef} data-account-switcher className={styles.form} onSubmit={event => {
        event.preventDefault(); const data = new FormData(event.currentTarget);
        if(stale==='changed'&&String(data.get('email')).toLowerCase()===original.current.email.toLowerCase())returning.current=true;
        void operation('/sign-in', { email: String(data.get('email')), password: String(data.get('password')), ...(accounts ? { revision: accounts.revision } : {}) });
      }}>
        <p>{t('เพิ่มบัญชีเพื่อสลับใช้งานบนอุปกรณ์นี้ได้ โดยใช้วิธีลงชื่อเข้าใช้ที่บัญชีได้รับอนุญาต', 'Sign in once to keep another account available on this device.')}</p>
        <Field appearance="standard">{t('อีเมล', 'Work email')}<TextInput appearance="standard" type="email" name="email" autoComplete="username" required disabled={busy}/></Field>
        <Field appearance="standard">{t('รหัสผ่าน', 'Password')}<TextInput appearance="standard" type="password" name="password" autoComplete="current-password" required disabled={busy}/></Field>
        {error && <p role="alert" className={styles.error}>{error}</p>}
        <div className={styles.formActions}><Button appearance="standard" variant="primary" type="submit" disabled={busy}><LogIn size={17}/>{t('เพิ่มบัญชีและใช้งาน', 'Sign in and use account')}</Button><Button appearance="standard" variant="ghost" type="button" disabled={busy} onClick={() => { formRef.current?.reset(); setAdding(false); }}>{t('ยกเลิก', 'Cancel')}</Button></div>
        <details><summary>{t('ใช้ Directory ของบริษัท', 'Use Company Directory')}</summary>
          <Field appearance="standard">{t('ลิงก์ลงชื่อเข้าใช้ Account ที่บริษัทให้','Company Account sign-in link')}<TextInput appearance="standard" name="directorySignIn" type="url" disabled={busy}/></Field>
          <Button appearance="standard" variant="ghost" type="button" disabled={busy} onClick={()=>{
            const link=formRef.current?.querySelector<HTMLInputElement>('input[name="directorySignIn"]')?.value??'',destination=deviceDirectoryLogin(link,accountOrigin,audience,windowPath(),holdingHostForHost(window.location.host)?.holdingSlug);
            if(!destination){setError(t('ใช้ลิงก์ลงชื่อเข้าใช้ Account ของบริษัทที่ได้รับอนุญาต','Enter the approved Company Account sign-in link.'));return;}
            if(!guard())return;formRef.current?.reset();setContextSwitching(true);window.location.assign(destination);
          }}>{t('ดำเนินการยืนยันกับบริษัท','Continue to corporate sign-in')}</Button>
        </details>
      </form>
    </Dialog>
    <Dialog appearance="standard" open={unverified} dismissDisabled onClose={()=>{}} title={t('กำลังรอตรวจผลการเปลี่ยนบัญชี','Verify the account change')}>
      <p role="alert">{error}</p><Button appearance="standard" disabled={checkingOutcome} onClick={async()=>{
        setCheckingOutcome(true);try{await accountRead();await checkAccount();setUnverified(false);}catch{setError(t('ยังตรวจผลไม่ได้ กรุณาตรวจการเชื่อมต่อแล้วลองตรวจอีกครั้ง','The outcome is still unavailable. Check your connection and verify again.'));}finally{setCheckingOutcome(false);}
      }}>{t('ตรวจสถานะอีกครั้ง','Check account status')}</Button>
    </Dialog>
    <Dialog appearance="standard" open={stale !== null && !busy&&!adding&&!unverified} onClose={() => { if (stale === 'unavailable') setStale(null); }} title={stale === 'changed' ? t('บัญชีเปลี่ยนหรือหมดอายุ', 'Your account changed or expired') : t('ตรวจสอบบัญชีไม่ได้', 'Could not verify your account')}>
      <p>{stale === 'changed' ? t('งานที่ค้างของบัญชีเดิมถูกระงับไว้ เลือกกลับบัญชีเดิม หรือเปิดต่อด้วยบัญชีปัจจุบัน', 'Work belonging to the previous account is paused. Return to that account, or continue with the current account.') : t('ข้อมูลยังอยู่ในหน้านี้ กรุณาตรวจการเชื่อมต่อแล้วลองอีกครั้งก่อนส่งรายการ', 'Your work is still here. Check your connection and verify your account before submitting.')}</p>
      {error && <p role="alert">{error}</p>}
      <div className={styles.formActions}>
        {stale === 'changed' ? <>
          {returnHandle && accounts?.accounts.some(account => account.handle === returnHandle && !account.current) && <Button appearance="standard" disabled={resumeUrl!==null} onClick={() => { setContextSwitching(false);returning.current=true; if(accounts)void operation('/active',{handle:returnHandle,revision:accounts.revision}); }}>{t('กลับบัญชีเดิม', 'Return to previous account')}</Button>}
          <Button appearance="standard" variant="primary" disabled={resumeUrl!==null} onClick={() => {
            // The central switch already succeeded. An old pending-command
            // blocker cannot force the newly selected user to retain that view.
            // Explicitly discard it and perform fresh SSO; never replay it.
            if((hasUnsavedWork()||contextSwitchBlockReason())&&!window.confirm(t('ทิ้งฟอร์มเดิมและใช้บัญชีปัจจุบันหรือไม่? รายการที่ส่งไปแล้วจะไม่ถูกส่งซ้ำ','Discard the previous draft and use the current account? Submitted requests will not be replayed.')))return;
            setContextSwitching(true);continueHere();
          }}>{t('ทิ้งฟอร์มและใช้บัญชีปัจจุบัน', 'Discard draft and use current account')}</Button>
          <Button appearance="standard" variant="ghost" disabled={resumeUrl!==null} onClick={() => { setContextSwitching(false); value.add(); }}>{t('เพิ่มบัญชี', 'Add account')}</Button>
          {accounts?.accounts.filter(account=>!account.current&&account.handle!==returnHandle).map(account=><Button appearance="standard" variant="ghost" key={account.handle} disabled={resumeUrl!==null} onClick={()=>{setContextSwitching(false);void operation('/active',{handle:account.handle,revision:accounts.revision});}}>{account.name}</Button>)}
        </> : <Button appearance="standard" onClick={() => { void check(); }}>{t('ตรวจอีกครั้ง', 'Check again')}</Button>}
      </div>
    </Dialog>
    {resumeUrl&&<iframe ref={resumeFrame} src={resumeUrl} title={t('คืน session ของบัญชีเดิม','Restore previous account session')} style={{display:'none'}}/>}
  </Context>;
}
function windowPath() { return typeof window === 'undefined' ? '/' : window.location.pathname + window.location.search; }

export function ProductAccountTrigger(props: Placement) {
  const menu = useProductAccount(), id = useId(), ready = useHydrated(),preferences=useAccountPreferences();
  if (!menu) return null;
  const t = (thai: string, english: string) => (props.language ? props.language === 'th' : menu.th) ? thai : english;
  const { user } = menu;
  return <Popover open={menu.owner === id} onOpenChange={open => { if (open) menu.open(id, props); else menu.close(); }}>
    <PopoverTrigger render={<Button appearance="standard" variant="plain" className={`${styles.trigger} ${props.sidebar ? styles.sidebarTrigger : ''}`} disabled={!ready || menu.busy} aria-label={t(`บัญชีและบทบาทของ ${user.name}`, `Account and roles for ${user.name}`)} />}>
      <PersonAvatar name={user.name} identity={user.email} small/><span className={styles.identity}><strong>{user.name}</strong><small>{props.roleLabel ?? menu.roles?.roles.find(role=>role.selected)?.label ?? menu.roles?.provider?.choices.find(role=>role.id===menu.roles?.provider?.selectedId)?.label ?? t('บัญชีและบทบาท', 'Account and roles')}</small></span><ChevronDown size={15} aria-hidden="true"/>
    </PopoverTrigger>
    <PopoverContent className={styles.menu} aria-label={t('บัญชีและบทบาท', 'Account and roles')}>
      <div data-account-switcher>
        <div className={styles.heading}><UserRound size={18}/><div><strong>{user.name}</strong><small>{user.email}</small></div></div>
        <h2>{t('บัญชีบนอุปกรณ์นี้', 'Accounts on this device')}</h2>
        <div className={styles.accounts}>{menu.accounts?.accounts.map(account => <div className={styles.accountRow} key={account.handle}>
          <Button appearance="standard" variant="plain" className={styles.accountChoice} disabled={menu.busy || account.current} aria-pressed={account.current} onClick={() => menu.choose(account.handle)}>
            <PersonAvatar name={account.name} identity={account.email} small/><span className={styles.identity}><strong>{account.name}</strong><small>{account.email}</small></span>{account.current && <Check size={17} aria-label={t('บัญชีปัจจุบัน','Current account')}/>}
          </Button>
          <Button appearance="standard" variant="plain" disabled={menu.busy} aria-label={t(`นำ ${account.email} ออกจากอุปกรณ์นี้`, `Remove ${account.email} from this device`)} onClick={() => menu.remove(account.handle)}><X size={15}/></Button>
        </div>)}</div>
        <Button appearance="standard" variant="ghost" disabled={menu.busy} onClick={menu.add}><Plus size={17}/>{t('เพิ่มบัญชี', 'Add account')}</Button>
        <div className={styles.roles}>
          <h2><ShieldCheck size={17}/>{t('บทบาทงาน', 'Work roles')}</h2>
          <Field appearance="standard">{t('บริษัท / พื้นที่ทำงาน', 'Company / workspace')}<Select appearance="standard" value={menu.workspace} disabled={menu.loading || menu.busy} onChange={event => menu.setWorkspace(event.target.value)}>
            <option value="">{t('เลือกพื้นที่ทำงาน', 'Choose a workspace')}</option>
            {menu.roles?.workspaces.map(workspace => <option key={workspace.key} value={workspace.key}>{workspace.label}</option>)}
          </Select></Field>
          {menu.loading && <p role="status">{t('กำลังตรวจบทบาท…', 'Checking roles…')}</p>}
          {menu.roles?.workspace && <Field appearance="standard">{t('บทบาท', 'Role')}<Select appearance="standard" value={menu.roles.roles.find(role => role.selected)?.id ?? ''} disabled={menu.loading || menu.busy} onChange={event => { const role = menu.roles?.roles.find(role => role.id === event.target.value); if (role) menu.chooseRole(role); }}>
            <option value="" disabled>{t('เลือกบทบาท', 'Choose a role')}</option>{menu.roles.roles.map(role => <option key={role.id} value={role.id}>{role.label}</option>)}
          </Select></Field>}
          {menu.roles?.provider && <Field appearance="standard">Platform / Provider<Select appearance="standard" value={menu.roles.provider.selectedId} disabled={menu.loading || menu.busy} onChange={event => { const role = menu.roles?.provider?.choices.find(choice => choice.id === event.target.value); if (role) menu.chooseRole({ ...role, selected: false }, true); }}>
            {menu.roles.provider.choices.map(role => <option key={role.id} value={role.id}>{role.label}</option>)}
          </Select></Field>}
        </div>
        {menu.error && <p role="alert" className={styles.error}>{menu.error}</p>}
        <Appearance appearance="standard" initialTheme={preferences?.preference?.appearance??'system'}/>
        <div className={styles.links}><a data-product-switch href={menu.accountOrigin+'/my-profile'}>{t('โปรไฟล์', 'My profile')}</a><a data-product-switch href={menu.accountOrigin+'/my-access'}>{t('สิทธิ์ของฉัน','My access')}</a><a data-product-switch href={menu.accountOrigin+'/security'}>{t('ความปลอดภัย','Security')}</a><a data-product-switch href={menu.appsHref}>Apps</a></div>
      </div>
    </PopoverContent>
  </Popover>;
}
