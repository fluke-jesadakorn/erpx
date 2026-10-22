"use client";

import { useState } from "react";
import {useAccountPreferences} from "@erpx/feature-identity_access/components/account-preferences-context";
import { Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";
import { PreferredDateTimeField } from "@erpx/feature-people_payroll/components/time/preferred-date-time-field";
import type { FieldWorkspace } from "@erpx/server-field_service/public/field-service";

export function ServiceTechnicianField({ source, branchId, departmentId, defaultUserId = "" }: {
  source: FieldWorkspace; branchId: string; departmentId: string | null; defaultUserId?: string;
}) {
  const people = source.people.filter(person=>person.branch_id===branchId && person.department_id===departmentId);
  return <Field appearance="standard">Technician<Select appearance="standard" name="technicianId" required
    defaultValue={people.some(person=>person.id===defaultUserId)?defaultUserId:""}>
    <option value="">Choose a technician</option>
    {people.map(person=><option key={person.id} value={person.id}>{person.name}</option>)}
  </Select></Field>;
}

export function ServiceAgreementReferenceFields({ source }: { source: FieldWorkspace }) {
  const th=useAccountPreferences()?.preference?.language==='th',t=(thai:string,english:string)=>th?thai:english;
  const branches = source.nodes.filter(node=>node.kind!=="department");
  const [branchId,setBranchId] = useState(source.referenceScope.branchId ?? branches[0]?.id ?? "");
  const [departmentId,setDepartmentId] = useState(source.referenceScope.departmentId ?? "");
  const [customerId,setCustomerId] = useState(source.customers[0]?.id ?? "");
  const [assetId,setAssetId] = useState("");
  const selectedCustomerId=customerId||source.customers[0]?.id||"",currency = source.customers.find(customer=>customer.id===selectedCustomerId)?.currency ?? "";
  return <>
    <Field appearance="standard">Branch<Select appearance="standard" name="branchId" required value={branchId}
      onChange={event=>{setBranchId(event.target.value);setDepartmentId(source.referenceScope.departmentId??"");setAssetId("");}}>
      {branches.map(node=><option key={node.id} value={node.id}>{node.display_name}</option>)}
    </Select></Field>
    <Field appearance="standard">Department<Select appearance="standard" name="departmentId" value={departmentId}
      required={Boolean(source.referenceScope.departmentId)} onChange={event=>setDepartmentId(event.target.value)}>
      {!source.referenceScope.departmentId && <option value="">No department</option>}
      {source.nodes.filter(node=>node.kind==="department"&&node.branch_id===branchId).map(node=><option key={node.id} value={node.id}>{node.display_name}</option>)}
    </Select></Field>
    <Field appearance="standard">Customer<Select appearance="standard" name="customerId" required value={selectedCustomerId} onChange={event=>setCustomerId(event.target.value)}>
      {!source.customers.length&&<option value="">{t('เตรียมลูกค้างานบริการก่อน','Prepare a Service customer first')}</option>}
      {source.customers.map(customer=><option key={customer.id} value={customer.id}>{customer.name}</option>)}
    </Select></Field>
    {!source.customers.length&&<p>{t('เตรียมลูกค้าด้านบน แล้วเปิดข้อตกลงนี้อีกครั้งเพื่อเลือกลูกค้าที่บันทึกแล้ว','Prepare a customer above, then reopen this agreement to choose the saved customer.')}</p>}
    {source.canUseAssetReferences && <Field appearance="standard">Asset<Select appearance="standard" name="assetId" value={assetId} onChange={event=>setAssetId(event.target.value)}>
      <option value="">Any covered asset</option>
      {source.assets.filter(asset=>asset.branch_id===branchId).map(asset=><option key={asset.id} value={asset.id}>{asset.code} · {asset.name}</option>)}
    </Select></Field>}
    <Field appearance="standard">Currency<TextInput appearance="standard" name="currency" value={currency} readOnly required /></Field>
  </>;
}

export function ServiceRequestReferenceFields({ source }: { source: FieldWorkspace }) {
  const agreements = source.agreements.filter(row=>Number(row.can_request)>0);
  const [selection,setSelection] = useState(agreements[0]?.id??"");
  const agreement = agreements.find(row=>row.id===selection);
  return <>
    <Field appearance="standard">Service agreement<Select appearance="standard" name="agreementId" required value={selection} onChange={event=>setSelection(event.target.value)}>
      {!agreements.length && <option value="">No eligible active agreement</option>}
      {agreements.map(row=><option key={row.id} value={row.id}>{String(row.title)} · {String(row.customer_name)}</option>)}
    </Select></Field>
    <Field appearance="standard">Request title<TextInput appearance="standard" name="title" required minLength={3} maxLength={160}/></Field>
    <Field appearance="standard">What needs attention?<TextArea appearance="standard" name="issue" required minLength={3}/></Field>
    <Field appearance="standard">Visit location<TextInput appearance="standard" name="site" required minLength={3}/></Field>
    <ServiceTechnicianField key={selection} source={source} branchId={String(agreement?.branch_id??"")} departmentId={agreement?.department_id?String(agreement.department_id):null}/>
    <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule"/>
  </>;
}

export function ServiceOrderReferenceFields({ source }: { source: FieldWorkspace }) {
  const [ticketId,setTicketId] = useState(String(source.tickets[0]?.id??""));
  const [selection,setSelection] = useState("");
  const ticket = source.tickets.find(row=>row.id===ticketId);
  const agreements = source.agreements.filter(row=>Number(row.can_request)>0 && row.customer_id===ticket?.customer_id
    && row.branch_id===ticket?.branch_id && row.department_id===ticket?.department_id);
  const agreement = agreements.find(row=>row.id===selection) ?? agreements[0];
  return <>
    <Field appearance="standard">Support case<Select appearance="standard" name="ticketId" required value={ticketId} onChange={event=>{setTicketId(event.target.value);setSelection("");}}>
      {!source.tickets.length && <option value="">No eligible source case</option>}
      {source.tickets.map(row=><option key={String(row.id)} value={String(row.id)}>{String(row.title)}</option>)}
    </Select></Field>
    <Field appearance="standard">Active agreement<Select appearance="standard" name="agreementId" required value={agreement?.id??""} onChange={event=>setSelection(event.target.value)}>
      {!agreements.length && <option value="">No matching active agreement</option>}
      {agreements.map(row=><option key={row.id} value={row.id}>{String(row.title)}</option>)}
    </Select></Field>
    {source.canUseAssetReferences && !agreement?.asset_id && <Field appearance="standard">Asset<Select key={agreement?.id??"none"} appearance="standard" name="assetId" defaultValue="">
      <option value="">No linked asset</option>
      {source.assets.filter(asset=>asset.branch_id===ticket?.branch_id).map(asset=><option key={asset.id} value={asset.id}>{asset.code} · {asset.name}</option>)}
    </Select></Field>}
    <Field appearance="standard">Issue<TextArea appearance="standard" name="issue" required minLength={3}/></Field>
    <Field appearance="standard">Site<TextInput appearance="standard" name="site" required minLength={3}/></Field>
    <ServiceTechnicianField key={ticketId} source={source} branchId={String(ticket?.branch_id??"")} departmentId={ticket?.department_id?String(ticket.department_id):null}/>
    <PreferredDateTimeField appearance="standard" name="scheduledAt" label="Schedule"/>
  </>;
}
