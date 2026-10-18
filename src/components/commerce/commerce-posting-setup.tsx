'use client';
import {useEffect,useLayoutEffect,useRef,useState} from 'react';
import {useRouter} from '@/lib/shared/destination-router';
import WorkspaceLink from '@/components/workplace/workspace-link';
import {Button,Card,Field,InlineNotice,Select,TextArea,TextInput} from '@/components/ui/foundation';
import {DraftFormDrawer} from '@/components/ui/draft-form-drawer';
import {CommerceDraftBoundary,CommerceForm} from '@/components/commerce/commerce-draft-boundary';
import {PostingSetupPrerequisites} from './posting-setup-prerequisites';
import {useAccountPreferences} from '@/components/identity_access/account-preferences-context';
import {createPostingSetupCommands,postingSetupViewState,type SavedPostingSetup} from '@/lib/commerce/posting-setup-command';
import {registerUnsavedWork} from '@/lib/work_projection/context-switch';
import type {CommercePostingSetupInput,CommercePostingSetupView} from '@/server/modules/commerce/public/posting-setup';
import type {SupplyPostingPrerequisitesView} from '@/server/modules/finance/public/supply-posting-prerequisites';

const accountFields=[
 ['receivableAccountId','ลูกหนี้การค้า','Receivables'],['payableAccountId','เจ้าหนี้การค้า','Payables'],['revenueAccountId','รายได้','Revenue'],
 ['inventoryAccountId','สินค้าคงเหลือ','Inventory'],['clearingAccountId','บัญชีพักรับสินค้า','Receipt clearing'],['cogsAccountId','ต้นทุนขาย','Cost of goods sold'],
 ['inputTaxAccountId','ภาษีซื้อ','Input tax'],['outputTaxAccountId','ภาษีขาย','Output tax'],['cashAccountId','เงินสด / ธนาคาร','Cash / bank'],
] as const;
type Draft=Omit<CommercePostingSetupInput,'expectedVersion'|'contextKey'>;
const emptyDraft=():Draft=>({chartId:'',currency:'',reason:'',receivableAccountId:'',payableAccountId:'',revenueAccountId:'',inventoryAccountId:'',clearingAccountId:'',cogsAccountId:'',inputTaxAccountId:'',outputTaxAccountId:'',cashAccountId:''});
const emptyStamp=JSON.stringify(emptyDraft());
function useCopy(){const language=useAccountPreferences()?.preference?.language;return (th:string,en:string)=>language==='th'?th:en;}
export function CommercePostingSetupRecovery({base}:{base:string}){
 const t=useCopy();return <Card appearance="standard" as="section"><h1>{t('ต้องตั้งค่าบัญชีสำหรับการจัดซื้อ','Purchasing posting setup required')}</h1><p>{t('บริษัทนี้ยังไม่มีผังบัญชีสำหรับการรับสินค้าและออกเอกสาร ผู้มีสิทธิ์การเงินระดับบริษัทสามารถเลือกบัญชีจริงทั้งเก้าบัญชีได้ในหน้าตั้งค่า','This Company has no posting mapping for receipts and documents. A Company Finance operator can review and select all nine actual accounts on the setup page.')}</p><WorkspaceLink href={`${base}/purchasing/setup/posting`}>{t('ตรวจสอบการตั้งค่าบัญชี','Review posting setup')}</WorkspaceLink></Card>;
}
export function CommercePostingSetup({base,initial,initialPrerequisites}:{base:string;initial:CommercePostingSetupView;initialPrerequisites?:SupplyPostingPrerequisitesView}){
 return <CommerceDraftBoundary><PostingSetupEditor base={base} initial={initial} initialPrerequisites={initialPrerequisites}/></CommerceDraftBoundary>;
}
function PostingSetupEditor({base,initial:serverInitial,initialPrerequisites}:{base:string;initial:CommercePostingSetupView;initialPrerequisites?:SupplyPostingPrerequisitesView}){
 const [refreshed,setRefreshed]=useState<{base:string;serverInitial:CommercePostingSetupView;value:CommercePostingSetupView;prerequisites:SupplyPostingPrerequisitesView}|null>(null),currentRefresh=refreshed?.base===base&&refreshed.serverInitial===serverInitial&&refreshed.value.companyId===serverInitial.companyId?refreshed:null,initial=currentRefresh?.value??serverInitial,prerequisites=currentRefresh?.prerequisites??initialPrerequisites;
 const [prerequisiteActive,setPrerequisiteActive]=useState(false);
 const t=useCopy(),router=useRouter(),trigger=useRef<HTMLButtonElement>(null),formRef=useRef<HTMLFormElement>(null),busyRef=useRef(false),source=useRef({base,companyId:initial.companyId,contextKey:initial.contextKey});
 useLayoutEffect(()=>{source.current={base,companyId:initial.companyId,contextKey:initial.contextKey};},[base,initial.companyId,initial.contextKey]);
 const commands=useRef<ReturnType<typeof createPostingSetupCommands>|null>(null);commands.current??=createPostingSetupCommands(()=>source.current);
 const [draft,setDraft]=useState(emptyDraft),[seed,setSeed]=useState(initial.contextKey),[draftScope,setDraftScope]=useState(()=>JSON.stringify([base,initial.companyId,initial.contextKey]));
 const [open,setOpen]=useState(false),[busy,setBusy]=useState(false),[uncertain,setUncertain]=useState(false),[error,setError]=useState(''),[notice,setNotice]=useState(''),[saved,setSaved]=useState<SavedPostingSetup|null>(null);
 const dirty=JSON.stringify(draft)!==emptyStamp||uncertain,view=postingSetupViewState({base,companyId:initial.companyId,contextKey:initial.contextKey},{saved,notice,seed,draftScope,dirty,pending:busy||uncertain}),currentScope=view.currentScope,changed=view.draftScope!==currentScope;
 useEffect(()=>registerUnsavedWork(()=>dirty),[dirty]);
 const canStart=initial.available&&initial.canConfigure&&!initial.configured&&!view.saved&&!changed&&!prerequisiteActive&&prerequisites?.unavailableReason!=='history-requires-review',configuration=initial.configuration??view.saved;
 const chart=initial.charts.find(item=>item.id===draft.chartId),accounts=initial.accounts.filter(item=>item.chartId===draft.chartId);
 const complete=!!chart&&/^[A-Z]{3}$/.test(draft.currency)&&draft.reason.trim().length>=3&&accountFields.every(([key])=>accounts.some(item=>item.id===draft[key]));
 const changeDraft=(update:(previous:Draft)=>Draft)=>{setSeed(view.seed);setDraftScope(view.draftScope);setDraft(update);};
 const change=(key:keyof Draft,value:string)=>changeDraft(previous=>({...previous,[key]:value}));
 function discard(){if(commands.current?.pending()){setError(t('ผลของคำขอเดิมยังไม่แน่นอน กรุณาลองคำขอเดิมก่อน','The submitted setup is unresolved. Retry that exact setup before discarding.'));return;}setDraft(emptyDraft());setSeed(initial.contextKey);setDraftScope(currentScope);setError('');setNotice('');setOpen(false);}
 async function submit(form:HTMLFormElement,retry=false){
  if(busyRef.current)return;busyRef.current=true;setBusy(true);setError('');setNotice('');
  try{
   if(!retry&&!canStart)throw Error(t('บริบทหรือสิทธิ์เปลี่ยน เก็บร่างไว้แล้ว กรุณาตรวจสอบการตั้งค่าอีกครั้ง','Your setup context changed. Your draft is retained; review the current setup.'));
   const completion={form,formEditVersion:form.getAttribute('data-erpx-edit-version')};
   const outcome=await(retry?commands.current!.retry():commands.current!.submit({...draft,expectedVersion:0,contextKey:view.seed??''},completion)),unchanged=outcome.unchanged();
   if(!outcome.acknowledge())throw Error(t('ต้องลองคำขอตั้งค่าเดิมอีกครั้ง','Retry the submitted posting setup.'));
   setSaved({base,companyId:initial.companyId,input:outcome.submitted});setUncertain(false);
   if(unchanged){setDraft(emptyDraft());setOpen(false);router.refresh();}else setNotice(t('บันทึกคำขอเดิมแล้ว การเลือกใหม่ของคุณยังเก็บไว้ แต่ไม่สามารถแทนที่การตั้งค่าที่บันทึกแล้ว','The submitted mapping was saved. Your newer selections are retained locally and cannot replace the saved configuration.'));
  }catch(cause){setUncertain(commands.current!.pending());setError(cause instanceof Error?cause.message:t('บันทึกไม่สำเร็จ กรุณาลองคำขอเดิม','Unable to confirm setup. Retry the submitted setup.'));}
  finally{busyRef.current=false;setBusy(false);}
 }
 return <div className="ui-page-content"><header><h1>{t('บัญชีสำหรับการจัดซื้อและสินค้า','Purchasing & inventory posting setup')}</h1><p>{t('ตั้งค่าครั้งแรกระดับบริษัทเท่านั้น บัญชีที่บันทึกแล้วจะแสดงเพื่ออ่านโดยไม่เปลี่ยนการผูกบัญชี','Initial Company setup only. Existing posting mappings are shown read-only and cannot be rebound here.')}</p></header>
  <WorkspaceLink href={`${base}/purchasing/orders`}>{t('กลับไปใบสั่งซื้อ','Back to purchase orders')}</WorkspaceLink>
  {!initial.available&&<InlineNotice role="status">{initial.unavailableReason==='purchase-required'?t('ต้องมีแพ็กเกจรับสินค้าและการประเมินมูลค่าสินค้าที่ยังใช้งานได้ จึงจะเริ่มตั้งค่าใหม่ได้','Active receiving and inventory valuation purchases are required for initial setup.'):prerequisites?.available?t('การเลือกบัญชีการลงรายการต้องใช้สิทธิ์จัดเตรียมการเงิน คำสั่งเตรียมข้อมูลอื่นจะแสดงตามสิทธิ์จริงด้านล่าง','Editing the posting mapping requires Finance preparation authority. Other prerequisite actions are available below according to your current role.'):t('ต้องใช้สิทธิ์การเงินระดับบริษัทเพื่อเริ่มตั้งค่า ไม่มีการเปลี่ยนสิทธิ์ให้โดยอัตโนมัติ','Use an authorized Company Finance role to begin setup. No role is selected or granted automatically.')}</InlineNotice>}
  {prerequisites&&<PostingSetupPrerequisites base={base} initial={prerequisites} disabled={open||dirty||busy||uncertain} onActiveChange={setPrerequisiteActive} onPrepared={(value,current)=>setRefreshed({base,serverInitial,value,prerequisites:current})}/>}
  {initial.available&&!initial.configured&&!view.saved&&<InlineNotice role="status">{t('ยังไม่ได้ตั้งค่าบัญชีการลงรายการ','Posting accounts have not been configured.')}</InlineNotice>}
  {configuration&&<Card appearance="standard" as="section"><h2>{t('การตั้งค่าที่บันทึกแล้ว','Saved posting configuration')}</h2><p>{t('อ่านอย่างเดียว','Read-only')} · {configuration.currency}</p><dl className="ui-form-grid"><div><dt>{t('ผังบัญชี','Chart')}</dt><dd>{initial.charts.find(item=>item.id===configuration.chartId)?.name??configuration.chartId}</dd></div>{accountFields.map(([key,th,en])=>{const account=initial.accounts.find(item=>item.id===configuration[key]);return <div key={key}><dt>{t(th,en)}</dt><dd>{account?`${account.code} · ${account.name}`:configuration[key]}</dd></div>;})}</dl></Card>}
  {view.notice&&<InlineNotice role="status">{view.notice}</InlineNotice>}{error&&!open&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
  {(canStart||dirty||uncertain)&&<><Button ref={trigger} appearance="standard" type="button" disabled={prerequisiteActive} onClick={()=>setOpen(true)}>{t('เลือกบัญชีการลงรายการ','Select posting accounts')}</Button>
   <DraftFormDrawer appearance="standard" title={t('เลือกบัญชีการลงรายการ','Select posting accounts')} open={open} dirty={dirty} retainFormContents={dirty} dismissDisabled={busy} finalFocus={()=>trigger.current} onClose={()=>setOpen(false)} onDiscard={discard}>
    {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}{view.notice&&<InlineNotice role="status">{view.notice}</InlineNotice>}
    {(changed||!canStart)&&<InlineNotice role="status">{t('ร่างยังเก็บไว้ แต่ไม่สามารถเริ่มการตั้งค่าใหม่ในบริบทปัจจุบันได้','Your draft is retained. A new configuration cannot be submitted in this context.')}</InlineNotice>}
    <CommerceForm ref={formRef} onSubmit={event=>{event.preventDefault();void submit(event.currentTarget);}}>
     <fieldset disabled={busy||!canStart} style={{display:'contents'}}>
      <Field appearance="standard">{t('ผังบัญชี','Chart')}<Select appearance="standard" name="chartId" required value={draft.chartId} onChange={event=>{const chartId=event.target.value;changeDraft(previous=>({...previous,chartId,...Object.fromEntries(accountFields.map(([key])=>[key,'']))}));}}><option value="" disabled>{t('เลือกผังบัญชีจริง','Select an actual chart')}</option>{initial.charts.map(item=><option key={item.id} value={item.id}>{item.name} · {item.baseCurrency}</option>)}</Select></Field>
      <p className="muted">{t('การเปลี่ยนผังบัญชีต้องเลือกทั้งเก้าบัญชีใหม่ ไม่มีการเลือกบัญชีเริ่มต้นให้อัตโนมัติ','Changing the chart requires selecting all nine accounts again. No posting accounts are selected automatically.')}</p>
      <Field appearance="standard">{t('สกุลเงินเอกสาร','Document currency')}<TextInput appearance="standard" name="currency" required minLength={3} maxLength={3} pattern="[A-Z]{3}" value={draft.currency} onChange={event=>change('currency',event.target.value.toUpperCase())}/></Field>
      {accountFields.map(([key,th,en])=><Field key={key} appearance="standard">{t(th,en)}<Select appearance="standard" name={key} required disabled={!draft.chartId} value={draft[key]} onChange={event=>change(key,event.target.value)}><option value="" disabled>{t('เลือกบัญชีจริง','Select an actual account')}</option>{accounts.map(item=><option key={item.id} value={item.id}>{item.code} · {item.name} · {item.category} · {item.currency}</option>)}</Select></Field>)}
      <Field appearance="standard">{t('เหตุผลที่ตรวจสอบการตั้งค่านี้','Review reason')}<TextArea appearance="standard" name="reason" minLength={3} maxLength={1000} required value={draft.reason} onChange={event=>change('reason',event.target.value)}/></Field>
     </fieldset>
     <div className="ui-form-actions"><Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy||!canStart||!complete}>{busy?t('กำลังบันทึก…','Saving…'):t('บันทึกการตั้งค่าครั้งแรก','Save initial configuration')}</Button></div>
    </CommerceForm>
    {uncertain&&<Button appearance="standard" type="button" disabled={busy} onClick={()=>{if(formRef.current)void submit(formRef.current,true);}}>{t('ลองคำขอเดิม (เก็บร่างปัจจุบัน)','Retry submitted setup (keep current draft)')}</Button>}
    {error&&!uncertain&&!busy&&<Button appearance="standard" type="button" onClick={()=>router.refresh()}>{t('โหลดบัญชีปัจจุบันโดยเก็บร่างไว้','Reload current accounts and keep draft')}</Button>}
   </DraftFormDrawer></>}
 </div>;
}
