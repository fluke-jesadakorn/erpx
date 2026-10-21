import {test,expect,type BrowserContext,type Page} from '@playwright/test';
import {randomUUID} from 'node:crypto';
import {businessActor,businessCustomerActor,businessPosFront,businessFixture,businessOpen,businessReady,businessClick,businessRead,businessRequest,businessEvidence,closeBusiness,customerPortalAccountState,customerPortalEmailLink,navigateCustomerPortalMailLink} from "../../support/browser/engineering-business";
import {createEngineeringBrowserContext} from "../../support/browser/engineering-browser-evidence";

async function posClick(page:Page,operation:string,click:()=>Promise<void>){
 const suffix=`/pos/${operation.replace('.', '/')}`;
 const pending=page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname.endsWith(suffix));
 await click();const response=await pending,body=await response.json();expect(response.ok(),JSON.stringify(body)).toBe(true);return body.data as Record<string,unknown>;
}

async function fillPortalPassword(page:Page,password:string){
 try{
  await page.evaluate(value=>{
   const input=document.querySelector<HTMLInputElement>('input[type="password"]'),setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value')?.set;
   if(!input||!setter)throw new Error('PASSWORD_FIELD_REQUIRED');
   setter.call(input,value);input.dispatchEvent(new Event('input',{bubbles:true}));input.dispatchEvent(new Event('change',{bubbles:true}));
  },password);
 }catch{throw new Error('CUSTOMER_PORTAL_PASSWORD_ENTRY_FAILED');}
}

async function openCustomerAccessAction(page:Page,label:string,reason:string,action?:string){
 const drawer=page.getByTestId('record-detail-drawer'),opener=drawer.getByRole('button',{name:label,exact:true});
 await expect(opener).toBeVisible();await expect(opener).toBeEnabled();
 const form=page.locator('form[data-crm-draft]').filter({has:page.getByRole('button',{name:label,exact:true})});
 let opened=false;
 for(let attempt=0;attempt<3&&!opened;attempt++){
  await opener.click();
  try{await expect(form).toHaveCount(1,{timeout:3000});opened=true;}catch(error){if(attempt===2)throw error;}
 }
 if(action){const select=form.locator('select[name="action"]');await expect(select).toBeEnabled();await expect(select.locator(`option[value="${action}"]`)).toBeEnabled();await select.selectOption(action);}
 await form.locator('[name="reason"]').fill(reason);
 const submit=form.getByRole('button',{name:label,exact:true});await expect(submit).toBeEnabled();
 return submit;
}

test.use({trace:'off'});
test.describe('native retail and customer authority engineering workflows',()=>{
 test.skip(!process.env.ERPX_DEMO_RUN,'Requires the disposable normal-login browser simulation.');
 test.setTimeout(240_000);

 test('sells counted retail stock collects cash independently approves a partial restock refund and reviews the exact cash session',async({browser},info)=>{
  const loaded=await businessFixture(info),{fixture,state,db}=loaded,r=fixture.retail,contexts:BrowserContext[]=[],releaseScripts:Array<()=>void>=[];
  let holdScripts=true;
  try{
   const cashier=await businessActor(browser,info,loaded,'source','work'),reviewer=await businessActor(browser,info,loaded,'reviewer','pos?tab=history&queue=approval');
   contexts.push(cashier.context,reviewer.context);
   const frontPage=await cashier.context.newPage(),front=await businessPosFront(cashier.context,frontPage,loaded,cashier.userId);
   async function openFront(query=''){
    await frontPage.goto(`${front.url}${query?`?${query}`:''}`,{waitUntil:holdScripts?'commit':'domcontentloaded'});
    await expect(frontPage.locator('[data-work-stale]')).toHaveAttribute('data-work-stale','false');
    if(!holdScripts){
     const language=frontPage.getByRole('button',{name:/^(EN|ไทย)$/});
     await expect(language).toHaveAttribute('data-client-managed-ready','true');await expect(language).toBeEnabled();
     if(await frontPage.locator('html').getAttribute('lang')!=='en')await frontPage.getByRole('button',{name:'EN',exact:true}).click();
     await expect(frontPage.locator('html')).toHaveAttribute('lang','en');
    }
   }
   let heldScripts=0,earlyShiftPosts=0;const sensitiveGets:string[]=[];
   frontPage.on('request',request=>{const url=new URL(request.url());if(request.method()==='POST'&&url.pathname.endsWith('/pos/shift/open'))++earlyShiftPosts;if(request.method()==='GET'&&(url.searchParams.has('register')||url.searchParams.has('opening')))sensitiveGets.push(url.pathname);});
   await frontPage.route('**/_next/**',async route=>{if(holdScripts&&route.request().resourceType()==='script'){++heldScripts;await new Promise<void>(resolve=>releaseScripts.push(resolve));}await route.continue();});
   await openFront('tab=shifts');
   const openShiftName=/^(Open cash (shift|session)|เปิดกะเงินสด)$/;
   const opening=frontPage.locator('form').filter({has:frontPage.getByRole('button',{name:openShiftName})});
   await expect(opening).toHaveCount(1);
   await opening.locator('select[name="register"]').selectOption(r.registerId);await opening.locator('[name="opening"]').fill('100.00');
   const submit=opening.getByRole('button',{name:openShiftName});await expect(submit).toHaveAttribute('data-client-managed-ready','false');await expect(submit).toBeDisabled();
   const box=await submit.boundingBox();expect(box).not.toBeNull();await frontPage.mouse.click(box!.x+box!.width/2,box!.y+box!.height/2);await opening.locator('[name="opening"]').press('Enter');expect(earlyShiftPosts).toBe(0);expect(sensitiveGets).toEqual([]);expect(heldScripts).toBeGreaterThan(0);
   holdScripts=false;for(const release of releaseScripts.splice(0))release();await expect(submit).toHaveAttribute('data-client-managed-ready','true');await expect(submit).toBeEnabled();await expect(opening.locator('[name="opening"]')).toHaveValue('100.00');await frontPage.unroute('**/_next/**');
   const opened=await posClick(frontPage,'shift.open',()=>submit.click()),shiftId=String(opened.id);
   expect(earlyShiftPosts).toBe(1);expect(sensitiveGets).toEqual([]);
   await openFront();await businessReady(frontPage);
   await frontPage.getByRole('combobox',{name:'Register',exact:true}).selectOption(r.registerId);
   const product=frontPage.getByRole('button').filter({hasText:r.productName});await expect(product).toBeEnabled();
   const sale=await posClick(frontPage,'sale.create',()=>frontPage.getByRole('button',{name:/^(New sale|Open new sale)$/}).click()),saleId=String(sale.id);
   await product.click();const selection=frontPage.locator('form').filter({has:frontPage.getByRole('button',{name:/^Add to (cart|sale)$/})});
   await selection.locator('[name="quantity"]').fill('2');await posClick(frontPage,'sale.add',()=>selection.getByRole('button',{name:/^Add to (cart|sale)$/}).click());
   const current=await businessRead<{sale:{state:string;version:number;currency:string;service_mode:string;total_minor:number};lines:Array<{id:string;fulfillment_id:string;price_minor:number;quantity_milli:number}>}>(cashier.page,`pos?sale=${saleId}`);
   expect(current.sale).toMatchObject({currency:'THB',service_mode:'retail',total_minor:2500});expect(current.lines).toHaveLength(1);expect(current.lines[0]).toMatchObject({price_minor:1250,quantity_milli:2000});
   const line=current.lines[0];
   const payment=frontPage.locator('form').filter({has:frontPage.getByRole('button',{name:'Record payment',exact:true})});
   await payment.locator('[name="amount"]').fill('25.00');await payment.locator('[name="tender"]').fill('30.00');
   const paid=await posClick(frontPage,'payment.create',()=>payment.getByRole('button',{name:'Record payment',exact:true}).click());
   await posClick(frontPage,'sale.complete',()=>frontPage.getByRole('button',{name:'Complete sale',exact:true}).click());
   await openFront(`sale=${saleId}`);await expect(frontPage.getByText('Partial return / refund',{exact:true})).toBeVisible();
   const retainedReceipt=(await db.query('SELECT snapshot_json::text FROM pos_sale_documents WHERE sale_id=$1',[saleId])).rows[0];expect(retainedReceipt).toBeTruthy();
   expect((await db.query('SELECT quantity_milli::integer,value_minor::integer FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2',[r.warehouseId,r.productId])).rows[0]).toEqual({quantity_milli:1000,value_minor:333});
   await frontPage.getByText('Partial return / refund',{exact:true}).click();
   const refundForm=frontPage.locator('form').filter({has:frontPage.getByRole('button',{name:'Request independent approval',exact:true})}),refundReason=`Fictional retail return ${info.project.name}: one sealed notebook independently inspected.`;
   await refundForm.locator(`[name="qty:${line.id}"]`).fill('1');await refundForm.locator(`[name="restock:${line.id}"]`).check();await refundForm.locator('[name="shift"]').selectOption(shiftId);await refundForm.locator('[name="reason"]').fill(refundReason);
   const refund=await posClick(frontPage,'refund.request',()=>refundForm.getByRole('button',{name:'Request independent approval',exact:true}).click()),refundId=String(refund.id);
   const self=await businessRequest(cashier.page,'pos/refund/approve',{id:refundId,expectedVersion:1,reason:'Cashier attempts approval of the refund they requested.'});expect(self.status()).toBe(409);expect((await self.json()).error.code).toBe('POS_INDEPENDENT_REVIEW');
   await businessOpen(reviewer.page,`pos?tab=history&queue=approval&sale=${saleId}`);await businessReady(reviewer.page);
   const requestReason=reviewer.page.locator("#workspace-content").locator('#pos-refund-requests').getByText(refundReason,{exact:true});await expect(requestReason).toBeVisible();
   const review=requestReason.locator('../..').locator('form');await review.locator('[name="reason"]').fill('Different manager inspected the returned notebook and confirmed the exact 12.50 THB cash payout.');
   await posClick(reviewer.page,'refund.approve',()=>review.getByRole('button',{name:'Approve and confirm refund',exact:true}).click());
   expect((await db.query('SELECT state,amount_minor::integer,requested_by,approved_by FROM pos_refunds WHERE id=$1',[refundId])).rows[0]).toEqual({state:'completed',amount_minor:1250,requested_by:state.personas.sourceUserId,approved_by:state.personas.reviewerUserId});
   expect((await db.query('SELECT quantity_milli::integer,value_minor::integer FROM commerce_stock WHERE warehouse_id=$1 AND product_id=$2',[r.warehouseId,r.productId])).rows[0]).toEqual({quantity_milli:2000,value_minor:666});
   expect((await db.query('SELECT state,amount_minor::integer,refunded_minor::integer,tendered_minor::integer FROM pos_payments WHERE id=$1',[paid.id])).rows[0]).toEqual({state:'succeeded',amount_minor:2500,refunded_minor:1250,tendered_minor:3000});
   expect((await db.query('SELECT snapshot_json::text FROM pos_sale_documents WHERE sale_id=$1',[saleId])).rows[0]).toEqual(retainedReceipt);
   const journalIds=(await db.query("SELECT j.id,SUM(l.base_debit_minor)::text debit,SUM(l.base_credit_minor)::text credit FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=$1 AND j.source_type='pos' AND j.source_id=ANY($2::text[]) AND j.status='posted' GROUP BY j.id",[state.companyId,[saleId,line.fulfillment_id,refundId]])).rows;expect(journalIds).toHaveLength(7);for(const journal of journalIds)expect(journal.debit).toBe(journal.credit);
   const gl=(await db.query("SELECT a.inventory,a.cash,a.revenue,a.cogs FROM pos_sales s CROSS JOIN LATERAL (SELECT SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE l.account_id=s.accounts_json::jsonb->>'inventory')::text inventory,SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE l.account_id=s.accounts_json::jsonb->>'cash')::text cash,SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE l.account_id=s.accounts_json::jsonb->>'revenue')::text revenue,SUM(l.base_debit_minor-l.base_credit_minor) FILTER(WHERE l.account_id=s.accounts_json::jsonb->>'cogs')::text cogs FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.company_id=s.company_id AND j.source_type='pos' AND j.source_id=ANY($2::text[]) AND j.status='posted') a WHERE s.id=$1",[saleId,[saleId,line.fulfillment_id,refundId]])).rows[0];expect(gl).toEqual({inventory:'-333',cash:'1250',revenue:'-1250',cogs:'333'});
   const reversed=(await db.query("SELECT r.amount_units::text,r.original_source_id,r.fx_rate_id=original.fx_rate_id same_rate FROM money_transaction_snapshots r JOIN money_transaction_snapshots original ON original.company_id=r.company_id AND original.source_type=r.original_source_type AND original.source_id=r.original_source_id AND original.amount_kind=r.original_amount_kind WHERE r.source_type='pos_refund' AND r.source_id=$1 AND r.amount_kind='total'",[refundId])).rows[0];expect(reversed).toEqual({amount_units:'-1250',original_source_id:saleId,same_rate:true});
   await openFront('tab=shifts');await businessReady(frontPage);
   const close=frontPage.locator('article').filter({has:frontPage.getByRole('heading',{name:r.registerName,exact:true})}).locator('form').filter({has:frontPage.getByRole('button',{name:'Close cash shift',exact:true})});
   await close.getByRole('checkbox',{name:'Enter counted total',exact:true}).check();await close.getByLabel('Counted cash',{exact:true}).fill('112.50');await posClick(frontPage,'shift.close',()=>close.getByRole('button',{name:'Close cash shift',exact:true}).click());
   await businessOpen(reviewer.page,'pos?tab=shifts&queue=approval');await businessReady(reviewer.page);
   const variance=reviewer.page.locator("#workspace-content").locator(`#pos-cash-variance-${shiftId}`);await variance.locator('[name="reason"]').fill('Independent manager reconciles 100.00 float plus 25.00 receipt less 12.50 confirmed refund.');await posClick(reviewer.page,'shift.review',()=>variance.getByRole('button',{name:'Review and record variance',exact:true}).click());
   const shift=(await db.query('SELECT state,opening_minor::integer,expected_minor::integer,counted_minor::integer,variance_minor::integer,opened_by,closed_by,reviewed_by FROM pos_shifts WHERE id=$1',[shiftId])).rows[0];expect(shift).toEqual({state:'reviewed',opening_minor:10000,expected_minor:11250,counted_minor:11250,variance_minor:0,opened_by:state.personas.sourceUserId,closed_by:state.personas.sourceUserId,reviewed_by:state.personas.reviewerUserId});
   await businessOpen(reviewer.page,`pos?tab=history&queue=approval&sale=${saleId}`);await businessEvidence(reviewer.page,info,'retail-cash-refund-restored-stock-and-reviewed-session');
  }finally{holdScripts=false;for(const release of releaseScripts.splice(0))release();await closeBusiness(contexts,db);}
 });

 test.describe('customer portal invitation acceptance',()=>{
 test.setTimeout(480_000);
 test('independently approves one binding invites the verified account scopes its document and support evidence revokes access and retains immutable authority history',async({browser},info)=>{
  const loaded=await businessFixture(info),{fixture,db}=loaded,a=fixture.customerAuthority,contexts:BrowserContext[]=[],path=`customers-sales/customers/reviews/access/${a.bindingId}/overview`,portalApi='/api/customer-portal/simulation';
  try{
   const source=await businessActor(browser,info,loaded,'source',path),reviewer=await businessActor(browser,info,loaded,'reviewer',path),customer=await businessCustomerActor(browser,info,loaded,a.recipientUserId),unrelated=await businessCustomerActor(browser,info,loaded,a.unrelatedUserId);
   contexts.push(source.context,reviewer.context,customer.context,unrelated.context);if(customer.email!==a.recipientEmail)throw new Error('CUSTOMER_PORTAL_RECIPIENT_FIXTURE_MISMATCH');
   await expect(customer.page.getByRole('heading',{name:'No approved documents yet',exact:true})).toBeVisible();await expect(source.page.getByRole('button',{name:'Record independent decision',exact:true})).toHaveCount(0);
   if((await db.query('SELECT binding_id FROM customer_access_identities WHERE binding_id=$1',[a.bindingId])).rows.length!==0)throw new Error('CUSTOMER_PORTAL_IDENTITY_EXISTS_BEFORE_INVITATION');
   const self=await businessRequest(source.page,'customer-access',{id:a.bindingId,action:'approve',expectedVersion:1,reason:'The requester attempts to approve their own external authority.'});expect(self.status()).toBe(403);expect((await self.json()).error.code).toBe('INDEPENDENT_APPROVER_REQUIRED');
   const approval=await openCustomerAccessAction(reviewer.page,'Record independent decision','Different Sales reviewer verifies the normal account email and shares only this approved quotation.','approve');
   await businessClick(reviewer.page,'customer-access',()=>approval.click());
   await customer.page.reload({waitUntil:'domcontentloaded'});await expect(customer.page.getByRole('heading',{name:'No approved documents yet',exact:true})).toBeVisible();
   await businessOpen(source.page,path);const invite=await openCustomerAccessAction(source.page,'Invite verified customer to the Portal','Invite the verified customer to claim only this independently approved quotation.');
   const issued=await businessClick(source.page,'customer-access',()=>invite.click());
   if(issued.id!==a.bindingId||issued.state!=='queued'||'token'in issued||'tokenHash'in issued)throw new Error('CUSTOMER_PORTAL_INVITATION_RECEIPT_INVALID');
   const invitationLink=await customerPortalEmailLink(loaded,a.recipientEmail,{subject:'Your ERPX Customer Portal invitation',purpose:'invitation',deliveryKey:`customer-portal-invitation-${String(issued.invitationId)}`});
   try{await navigateCustomerPortalMailLink(customer.page,invitationLink);}catch{throw new Error('CUSTOMER_PORTAL_INVITATION_PAGE_UNAVAILABLE');}
   await expect(customer.page.getByRole('heading',{name:'Accept your Customer Portal invitation',exact:true})).toBeVisible();
   await expect.poll(()=>customer.page.evaluate(()=>window.location.hash==='')).toBe(true);
   const acceptance=customer.page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname==='/api/customer-portal/enroll');
   await customer.page.getByRole('button',{name:'Accept while signed in',exact:true}).click();const accepted=await acceptance;
   if(!accepted.ok())throw new Error('CUSTOMER_PORTAL_INVITATION_ACCEPTANCE_FAILED');
   const completion=(await accepted.json()).data as {portalUrl?:unknown};
   if(typeof completion.portalUrl!=='string')throw new Error('CUSTOMER_PORTAL_INVITATION_COMPLETION_INVALID');
   const baseUrl=process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104',portalUrl=new URL(completion.portalUrl,baseUrl);
   if(portalUrl.origin!==new URL(baseUrl).origin||portalUrl.pathname!=='/customer-portal/simulation')throw new Error('CUSTOMER_PORTAL_DESTINATION_INVALID');
   await expect(customer.page.getByRole('link',{name:'Open Customer Portal',exact:true})).toHaveAttribute('href',completion.portalUrl);
   await customer.page.getByRole('link',{name:'Open Customer Portal',exact:true}).click();await expect(customer.page.getByRole('heading',{name:'Customer documents',exact:true})).toBeVisible();
   const projectionResponse=await customer.page.request.get(portalApi);expect(projectionResponse.ok()).toBe(true);const projection=(await projectionResponse.json()).data.items as Array<Record<string,unknown>>;
   expect(projection).toHaveLength(1);expect(projection[0]).toMatchObject({binding_id:a.bindingId,binding_version:2,order_id:a.orderId});expect(projection.some(row=>row.order_id===a.excludedOrderId)).toBe(false);for(const field of ['reason','created_by','customer_id','subtotal_minor','accounts_json'])expect(projection[0]).not.toHaveProperty(field);
   await expect(customer.page.locator(`#customer-order-${a.bindingId}`)).toBeVisible();
   const wrong=await unrelated.page.request.post(portalApi,{headers:{Origin:new URL(unrelated.page.url()).origin,'Idempotency-Key':crypto.randomUUID()},data:{bindingId:a.bindingId,expectedBindingVersion:2,action:'request_support',note:'Another normal account tries to use this customer binding.'}});expect(wrong.status()).toBe(403);expect((await wrong.json()).error.code).toBe('CUSTOMER_PORTAL_BINDING_UNAVAILABLE');
   expect((await (await unrelated.page.request.get(portalApi)).json()).data.items).toEqual([]);
   const stale=await customer.page.request.post(portalApi,{headers:{Origin:new URL(customer.page.url()).origin,'Idempotency-Key':crypto.randomUUID()},data:{bindingId:a.bindingId,expectedBindingVersion:1,action:'request_support',note:'A cached stale version attempts a customer command.'}});expect(stale.status()).toBe(409);expect((await stale.json()).error.code).toBe('CUSTOMER_PORTAL_BINDING_CHANGED');
   await customer.page.getByText('Request support',{exact:true}).click();const evidence=customer.page.getByRole('region',{name:'Support request photos and documents',exact:true});await evidence.locator('input[type="file"]').first().setInputFiles({name:`customer-authority-${info.project.name}.png`,mimeType:'image/png',buffer:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64')});await expect(evidence).toContainText(/1\/\d+ ready/,{timeout:90_000});
   await customer.page.getByLabel('Message to the company (optional for support)',{exact:true}).fill('Fictional customer requests clarification of the approved quotation; scanned evidence belongs only to this source.');
   const pendingResponse=customer.page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname===portalApi),pendingRequest=customer.page.waitForRequest(request=>request.method()==='POST'&&new URL(request.url()).pathname===portalApi);
   const sendSupport=customer.page.getByRole('button',{name:'Send support request',exact:true});await sendSupport.scrollIntoViewIfNeeded();const sendSupportBox=await sendSupport.boundingBox();if(!sendSupportBox)throw new Error('CUSTOMER_PORTAL_SUPPORT_SUBMIT_UNAVAILABLE');await sendSupport.click({position:{x:Math.max(1,sendSupportBox.width-12),y:sendSupportBox.height/2},timeout:30_000});const [recorded,request]=await Promise.all([pendingResponse,pendingRequest]);expect(recorded.ok(),await recorded.text()).toBe(true);const action=(await recorded.json()).data,commandKey=String(request.headers()['idempotency-key']);
   const replay=await customer.page.request.post(portalApi,{headers:{Origin:new URL(customer.page.url()).origin,'Idempotency-Key':commandKey},data:request.postDataJSON()});expect(replay.ok(),await replay.text()).toBe(true);expect((await replay.json()).data).toEqual(action);
   const bound=(await db.query("SELECT s.state,l.state link_state,l.id link_id,f.source_state FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id JOIN attachments f ON f.id=l.attachment_id WHERE s.target_type='customer_access_binding' AND s.target_id=$1 AND s.state='bound'",[a.bindingId])).rows;expect(bound).toHaveLength(1);expect(bound[0]).toMatchObject({state:'bound',link_state:'bound',source_state:'available'});
   const preview=`${portalApi}/attachments/attachment-links/${bound[0].link_id}/content?variant=preview`,previewResponse=await customer.page.request.get(preview);expect(previewResponse.ok()).toBe(true);expect(previewResponse.headers()['content-type']).toBe('image/webp');expect((await previewResponse.body()).byteLength).toBeGreaterThan(0);expect((await customer.page.request.get(preview.replace('variant=preview','variant=original'))).status()).toBe(403);expect((await unrelated.page.request.get(preview)).status()).toBe(404);
   expect((await db.query('SELECT user_id,email FROM customer_access_identities WHERE binding_id=$1',[a.bindingId])).rows).toEqual([{user_id:a.recipientUserId,email:a.recipientEmail}]);
   expect((await db.query('SELECT COUNT(*)::integer count FROM customer_portal_actions WHERE binding_id=$1',[a.bindingId])).rows[0].count).toBe(1);expect((await db.query('SELECT COUNT(*)::integer count FROM audit_events WHERE command_id=$1',[commandKey])).rows[0].count).toBe(1);expect((await db.query('SELECT COUNT(*)::integer count FROM outbox_events WHERE command_id=$1',[commandKey])).rows[0].count).toBe(1);
   await customer.page.reload({waitUntil:'domcontentloaded'});await customer.page.getByText('Request support',{exact:true}).click();await expect(customer.page.getByRole('button',{name:'Support requested',exact:true})).toBeDisabled();await businessEvidence(customer.page,info,'scoped-customer-portal-support-with-scanned-evidence');const supportDrawer=customer.page.getByRole('dialog',{name:'Request support',exact:true});await supportDrawer.getByRole('button',{name:'Close dialog',exact:true}).click();await expect(supportDrawer).toBeHidden();
   await businessOpen(source.page,path);const revoke=await openCustomerAccessAction(source.page,'Revoke access','Fictional customer withdraws portal authority; retained source and customer action history must remain.');await businessClick(source.page,'customer-access',()=>revoke.click());
   const cachedResponse=customer.page.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname===portalApi);await customer.page.getByRole('button',{name:'Acknowledge delivery',exact:true}).click();const cached=await cachedResponse;expect(cached.status()).toBe(403);expect((await cached.json()).error.code).toBe('CUSTOMER_PORTAL_BINDING_EXPIRED');expect((await customer.page.request.get(preview)).status()).toBe(404);
   await customer.page.reload({waitUntil:'domcontentloaded'});await expect(customer.page.getByRole('heading',{name:'No approved documents yet',exact:true})).toBeVisible();expect((await (await customer.page.request.get(portalApi)).json()).data.items).toEqual([]);
   await businessOpen(source.page,path);const history=await businessRead<{binding:{status:string;version:number};events:Array<{action:string}>;reads:Array<{user_id:string;binding_version:number;projection_version:string}>;invitations:Array<{id:string;state:string;accepted_user_id:string|null}>;invitationEvents:Array<{invitation_id:string;action:string}>}>(source.page,`customer-access?id=${a.bindingId}`);expect(history.binding).toMatchObject({status:'revoked',version:4});expect(history.events.map(row=>row.action)).toEqual(['request','approve','portal-evidence-bind','revoke']);expect(history.reads.length).toBeGreaterThan(0);expect(history.reads.every(row=>row.user_id===a.recipientUserId&&[2,3].includes(Number(row.binding_version))&&row.projection_version==='customer-order-v1')).toBe(true);expect(history.invitations).toContainEqual(expect.objectContaining({state:'accepted',accepted_user_id:a.recipientUserId}));expect(history.invitationEvents.some(row=>row.action==='accepted')).toBe(true);
   expect((await db.query('SELECT COUNT(*)::integer count FROM customer_portal_actions WHERE binding_id=$1',[a.bindingId])).rows[0].count).toBe(1);await expect(source.page.getByRole('heading',{name:'Immutable decision history',exact:true})).toBeVisible();await expect(source.page.getByRole('heading',{name:'Customer read audit',exact:true})).toBeVisible();await businessEvidence(source.page,info,'revoked-customer-portal-retains-versioned-decisions-and-read-audit');
  }finally{await closeBusiness(contexts,db);}
 });
 });

 test.describe('external customer portal invitation signup',()=>{
  test.setTimeout(360_000);
  test('independently approves a binding verifies a newly created account accepts its one approved document revokes access and retains immutable invitation history',async({browser},info)=>{
   const loaded=await businessFixture(info),{fixture,db}=loaded,a=fixture.customerAuthority,contexts:BrowserContext[]=[],path=`customers-sales/customers/reviews/access/${a.invitedBindingId}/overview`,portalApi='/api/customer-portal/simulation';
   try{
    const source=await businessActor(browser,info,loaded,'source',path);contexts.push(source.context);
    const reviewer=await businessActor(browser,info,loaded,'reviewer',path);contexts.push(reviewer.context);
    const unrelated=await businessCustomerActor(browser,info,loaded,a.unrelatedUserId);contexts.push(unrelated.context);
    const inviteeContext=await createEngineeringBrowserContext(browser,info);contexts.push(inviteeContext);const invitee=await inviteeContext.newPage();
    expect(await customerPortalAccountState(loaded,a.invitedEmail)).toBeNull();
    expect((await db.query('SELECT binding_id FROM customer_access_identities WHERE binding_id=$1',[a.invitedBindingId])).rows).toEqual([]);

    const self=await businessRequest(source.page,'customer-access',{id:a.invitedBindingId,action:'approve',expectedVersion:1,reason:'The requester attempts to approve their own customer authority.'});expect(self.status()).toBe(403);expect((await self.json()).error.code).toBe('INDEPENDENT_APPROVER_REQUIRED');
    const approval=await openCustomerAccessAction(reviewer.page,'Record independent decision','A separate Sales reviewer approves one document for this external customer email.','approve');
    await businessClick(reviewer.page,'customer-access',()=>approval.click());
    await businessOpen(source.page,path);const invite=await openCustomerAccessAction(source.page,'Invite verified customer to the Portal','Issue a private invitation to the newly approved external customer contact.');
    const issued=await businessClick(source.page,'customer-access',()=>invite.click());
    if(issued.id!==a.invitedBindingId||issued.state!=='queued'||'token'in issued||'tokenHash'in issued||typeof issued.invitationId!=='string')throw new Error('CUSTOMER_PORTAL_INVITATION_RECEIPT_INVALID');
    const invitationLink=await customerPortalEmailLink(loaded,a.invitedEmail,{subject:'Your ERPX Customer Portal invitation',purpose:'invitation',deliveryKey:`customer-portal-invitation-${issued.invitationId}`});
    try{await navigateCustomerPortalMailLink(invitee,invitationLink);}catch{throw new Error('CUSTOMER_PORTAL_INVITATION_PAGE_UNAVAILABLE');}
    await expect(invitee.getByRole('heading',{name:'Accept your Customer Portal invitation',exact:true})).toBeVisible();await expect.poll(()=>invitee.evaluate(()=>window.location.hash==='')).toBe(true);
    const password=`Portal!${randomUUID()}`,signupStartedAt=Date.now(),signupResponse=invitee.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname==='/api/customer-portal/enroll');
    await invitee.getByLabel('Your name').fill('External Portal invitee');await fillPortalPassword(invitee,password);await invitee.getByRole('button',{name:'Create invited account',exact:true}).click();
    const signup=await signupResponse;if(!signup.ok()){const body=await signup.json().catch(()=>null) as {error?:{code?:unknown}}|null;throw new Error(`CUSTOMER_PORTAL_INVITED_SIGNUP_FAILED:${signup.status()}:${typeof body?.error?.code==='string'?body.error.code:'UNKNOWN'}`);}await expect(invitee.getByRole('status')).toContainText('Check the invited email address and verify your account');
    const unverified=await customerPortalAccountState(loaded,a.invitedEmail);expect(unverified).not.toBeNull();expect(unverified?.emailVerified).toBe(false);expect((await db.query('SELECT binding_id FROM customer_access_identities WHERE binding_id=$1',[a.invitedBindingId])).rows).toEqual([]);
    const verificationLink=await customerPortalEmailLink(loaded,a.invitedEmail,{subject:'Verify your ERPX email',purpose:'verification',createdAfter:signupStartedAt});
    try{await navigateCustomerPortalMailLink(invitee,verificationLink);}catch{throw new Error('CUSTOMER_PORTAL_EMAIL_VERIFICATION_UNAVAILABLE');}
    await expect.poll(async()=>{const current=new URL(invitee.url());return current.pathname==='/customer-portal/enroll'&&current.searchParams.get('verified')==='1';}).toBe(true);
    await expect(invitee.getByText('Your email verification step is complete. Reopen the invitation email and sign in to accept the approved document.',{exact:true})).toBeVisible();
    await expect.poll(async()=>Boolean((await customerPortalAccountState(loaded,a.invitedEmail))?.emailVerified),{timeout:20_000}).toBe(true);
    let accountSession=await invitee.request.get('/api/auth/get-session');let sessionBody=accountSession.ok()?await accountSession.json().catch(()=>null):null;
    if((sessionBody as {user?:{email?:string}}|null)?.user?.email!==a.invitedEmail){
     await invitee.goto('/login?next=%2Fcustomer-portal%2Fenroll',{waitUntil:'domcontentloaded'});await invitee.locator('input[type="email"]').fill(a.invitedEmail);await fillPortalPassword(invitee,password);
     const signedIn=invitee.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname==='/api/auth/sign-in/email');await invitee.getByRole('button',{name:'Sign in',exact:true}).click();if(!(await signedIn).ok())throw new Error('CUSTOMER_PORTAL_INVITED_ACCOUNT_SIGN_IN_FAILED');
     await expect.poll(async()=>{const current=new URL(invitee.url());return current.pathname==='/customer-portal/enroll';}).toBe(true);
     accountSession=await invitee.request.get('/api/auth/get-session');sessionBody=accountSession.ok()?await accountSession.json().catch(()=>null):null;
    }
    if((sessionBody as {user?:{email?:string}}|null)?.user?.email!==a.invitedEmail)throw new Error('CUSTOMER_PORTAL_VERIFIED_INVITEE_SESSION_REQUIRED');
    const invitedAccount=await customerPortalAccountState(loaded,a.invitedEmail);expect(invitedAccount).toMatchObject({emailVerified:true});if(!invitedAccount)throw new Error('CUSTOMER_PORTAL_INVITED_ACCOUNT_MISSING');

    try{await navigateCustomerPortalMailLink(invitee,invitationLink);}catch{throw new Error('CUSTOMER_PORTAL_INVITATION_REOPEN_FAILED');}
    await expect(invitee.getByRole('heading',{name:'Accept your Customer Portal invitation',exact:true})).toBeVisible();await expect.poll(()=>invitee.evaluate(()=>window.location.hash==='')).toBe(true);
    const acceptance=invitee.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname==='/api/customer-portal/enroll');await invitee.getByRole('button',{name:'Accept while signed in',exact:true}).click();const accepted=await acceptance;if(!accepted.ok()){const body=await accepted.json().catch(()=>null) as {error?:{code?:unknown}}|null;throw new Error(`CUSTOMER_PORTAL_INVITATION_ACCEPTANCE_FAILED:${accepted.status()}:${typeof body?.error?.code==='string'?body.error.code:'UNKNOWN'}`);}
    const acceptedBody=await accepted.json().catch(()=>null) as {data?:{bindingId?:unknown;portalUrl?:unknown}}|null;if(acceptedBody?.data?.bindingId!==a.invitedBindingId||typeof acceptedBody.data.portalUrl!=='string')throw new Error('CUSTOMER_PORTAL_INVITATION_COMPLETION_INVALID');
    const portalUrl=new URL(acceptedBody.data.portalUrl,process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104');if(portalUrl.origin!==new URL(process.env.ERPX_E2E_BASE_URL??'http://127.0.0.1:3104').origin||portalUrl.pathname!=='/customer-portal/simulation')throw new Error('CUSTOMER_PORTAL_DESTINATION_INVALID');
    await expect(invitee.getByRole('link',{name:'Open Customer Portal',exact:true})).toHaveAttribute('href',acceptedBody.data.portalUrl);await invitee.getByRole('link',{name:'Open Customer Portal',exact:true}).click();await expect(invitee.getByRole('heading',{name:'Customer documents',exact:true})).toBeVisible();
    const projectionResponse=await invitee.request.get(portalApi);expect(projectionResponse.ok()).toBe(true);const projection=(await projectionResponse.json()).data.items as Array<Record<string,unknown>>;expect(projection).toHaveLength(1);expect(projection[0]).toMatchObject({binding_id:a.invitedBindingId,binding_version:2,order_id:a.orderId});expect(projection.some(row=>row.order_id===a.excludedOrderId)).toBe(false);for(const field of ['reason','created_by','customer_id','subtotal_minor','accounts_json'])expect(projection[0]).not.toHaveProperty(field);await expect(invitee.locator(`#customer-order-${a.invitedBindingId}`)).toBeVisible();
    expect((await db.query('SELECT binding_id,user_id,email FROM customer_access_identities WHERE binding_id=$1',[a.invitedBindingId])).rows).toEqual([{binding_id:a.invitedBindingId,user_id:invitedAccount.id,email:a.invitedEmail}]);
    expect((await db.query('SELECT state,accepted_user_id FROM customer_portal_invitations WHERE id=$1',[issued.invitationId])).rows).toEqual([{state:'accepted',accepted_user_id:invitedAccount.id}]);
    const invitationEvents=(await db.query('SELECT action FROM customer_portal_invitation_events WHERE invitation_id=$1',[issued.invitationId])).rows as Array<{action:string}>;expect(invitationEvents.some(row=>row.action==='accepted')).toBe(true);
    const wrong=await unrelated.page.request.get(portalApi);expect(wrong.ok()).toBe(true);expect((await wrong.json()).data.items).toEqual([]);

     await invitee.getByText('Request support',{exact:true}).click();const evidence=invitee.getByRole('region',{name:'Support request photos and documents',exact:true});await evidence.locator('input[type="file"]').first().setInputFiles({name:`customer-invitation-${info.project.name}.png`,mimeType:'image/png',buffer:Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64')});await expect(evidence).toContainText(/1\/\d+ ready/,{timeout:90_000});
     await invitee.getByLabel('Message to the company (optional for support)',{exact:true}).fill('Fictional invited customer requests clarification; evidence is limited to this accepted customer binding.');
     const supportResponse=invitee.waitForResponse(response=>response.request().method()==='POST'&&new URL(response.url()).pathname===portalApi,{timeout:30_000}),supportRequest=invitee.waitForRequest(request=>request.method()==='POST'&&new URL(request.url()).pathname===portalApi,{timeout:30_000});
     const supportSubmit=invitee.getByRole('button',{name:'Send support request',exact:true});await expect(supportSubmit).toBeEnabled();const supportSubmitBox=await supportSubmit.boundingBox();if(!supportSubmitBox)throw new Error('CUSTOMER_PORTAL_SUPPORT_SUBMIT_UNAVAILABLE');await supportSubmit.click({position:{x:Math.max(4,supportSubmitBox.width-24),y:supportSubmitBox.height/2}});const [supported,supportCommand]=await Promise.all([supportResponse,supportRequest]);if(!supported.ok())throw new Error('CUSTOMER_PORTAL_INVITEE_SUPPORT_REQUEST_FAILED');
     const supportReceipt=(await supported.json()).data,commandKey=supportCommand.headers()['idempotency-key'];if(!commandKey)throw new Error('CUSTOMER_PORTAL_SUPPORT_COMMAND_KEY_MISSING');
     const replay=await invitee.request.post(portalApi,{headers:{Origin:new URL(invitee.url()).origin,'Idempotency-Key':commandKey},data:supportCommand.postDataJSON()});expect(replay.ok()).toBe(true);expect((await replay.json()).data).toEqual(supportReceipt);
     const attachment=(await db.query("SELECT l.id link_id FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE s.target_type='customer_access_binding' AND s.target_id=$1 AND s.state='bound' AND l.state='bound'",[a.invitedBindingId])).rows[0] as {link_id:string}|undefined;if(!attachment)throw new Error('CUSTOMER_PORTAL_INVITEE_EVIDENCE_NOT_BOUND');
     const preview=`${portalApi}/attachments/attachment-links/${attachment.link_id}/content?variant=preview`,previewResponse=await invitee.request.get(preview);expect(previewResponse.ok()).toBe(true);expect(previewResponse.headers()['content-type']).toBe('image/webp');expect((await previewResponse.body()).byteLength).toBeGreaterThan(0);expect((await invitee.request.get(preview.replace('variant=preview','variant=original'))).status()).toBe(403);expect((await unrelated.page.request.get(preview)).status()).toBe(404);

    await businessOpen(source.page,path);const revoke=await openCustomerAccessAction(source.page,'Revoke access','Withdraw the external customer grant while retaining the exact invitation decision history.');await businessClick(source.page,'customer-access',()=>revoke.click());
     await invitee.reload({waitUntil:'domcontentloaded'});await expect(invitee.getByRole('heading',{name:'No approved documents yet',exact:true})).toBeVisible();expect((await (await invitee.request.get(portalApi)).json()).data.items).toEqual([]);expect((await invitee.request.get(preview)).status()).toBe(404);
    await businessOpen(source.page,path);const history=await businessRead<{binding:{status:string;version:number};invitations:Array<{id:string;state:string;accepted_user_id:string|null}>;invitationEvents:Array<{invitation_id:string;action:string}>}>(source.page,`customer-access?id=${a.invitedBindingId}`);expect(history.binding).toMatchObject({status:'revoked',version:4});expect(history.invitations).toContainEqual(expect.objectContaining({id:issued.invitationId,state:'accepted',accepted_user_id:invitedAccount.id}));expect(history.invitationEvents.some(row=>row.invitation_id===issued.invitationId&&row.action==='accepted')).toBe(true);
   }finally{await closeBusiness(contexts,db);}
  });
 });
});
