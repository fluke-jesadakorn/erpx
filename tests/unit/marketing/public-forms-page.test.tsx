import {beforeEach,describe,expect,it,vi} from 'vitest';
import {Children,isValidElement,type ComponentProps} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import type {holdingBindingBySlug} from '@/server/modules/runtime/public/runtime';
import type {marketingPresentationQueries} from '@/server/modules/marketing/public/presentation-queries';

type Queries=Pick<Awaited<ReturnType<typeof marketingPresentationQueries>>,'loadPublicMarketingForm'>;
type Reader=Queries['loadPublicMarketingForm'];
const owner=vi.hoisted(()=>({
 binding:vi.fn<typeof holdingBindingBySlug>(),
 queries:vi.fn<(holdingId:string)=>Promise<Queries>>(),
 read:vi.fn<Reader>(),
}));
// Only the named owner ports are substituted. The real Page, Next notFound,
// ApiError, standard presentation and actual public consent form execute.
vi.mock('@/server/modules/runtime/public/runtime',()=>({holdingBindingBySlug:owner.binding}));
vi.mock('@/server/modules/marketing/public/presentation-queries',()=>({marketingPresentationQueries:owner.queries}));
import Page from '@/app/forms/[holdingSlug]/[formId]/page';
import {PublicMarketingForm} from '@/components/marketing/public-marketing-form';
import {ApiError} from '@/server/modules/runtime/public/http';

const target={holdingSlug:'original-holding',formId:'original-reviewed-form'};
const form:Awaited<ReturnType<Reader>>={
 id:target.formId,company_id:'original-company',name:'Original published inquiry',version:7,
 preview_json:JSON.stringify({headline:'Original public form headline'}),configuration_json:JSON.stringify({consentRequired:true}),
 presentation:{ready:true,blockers:[],content:{id:'reviewed-content',version:3,headline:'Approved content headline',summary:'Approved content summary'},media:{id:'approved-publication',altText:'Original approved creative',revision:2,mediaType:'image/png'}},
};
const page=()=>Page({params:Promise.resolve(target)});
const missing={digest:'NEXT_HTTP_ERROR_FALLBACK;404'};
beforeEach(()=>{
 vi.resetAllMocks();
 owner.binding.mockResolvedValue({id:'h001',slug:target.holdingSlug,displayName:'Original Holding'});
 owner.queries.mockResolvedValue({loadPublicMarketingForm:owner.read});
 owner.read.mockResolvedValue(form);
});

describe('public Forms Page owner error boundary',()=>{
 it('renders the named owner projection and actual consent form with its original native identity/version',async()=>{
  const node=await page(),rendered=renderToStaticMarkup(node);
  expect(owner.binding).toHaveBeenCalledExactlyOnceWith(target.holdingSlug);
  expect(owner.queries).toHaveBeenCalledExactlyOnceWith('h001');
  expect(owner.read).toHaveBeenCalledExactlyOnceWith(target.formId);
  for(const value of [form.name,'Original public form headline','Approved content headline','Approved content summary','Original approved creative'])expect(rendered).toContain(value);
  expect(rendered).toContain(`/api/public/holdings/${target.holdingSlug}/media-publications/approved-publication/content`);
  expect(rendered).toContain('aria-label="Approved form content"');
  expect(rendered).toContain('name="consent"');expect(rendered).toContain('Send request');
  const control=Children.toArray(node.props.children).find(child=>isValidElement(child)&&child.type===PublicMarketingForm);
  expect(isValidElement<ComponentProps<typeof PublicMarketingForm>>(control)).toBe(true);
  if(!isValidElement<ComponentProps<typeof PublicMarketingForm>>(control))throw Error('The actual public consent form is required.');
  expect(control.type).toBe(PublicMarketingForm);
  expect(control.props).toEqual({holdingSlug:target.holdingSlug,formId:form.id,formVersion:form.version});
 });
 it('keeps the original unknown-Holding sentinel as real Next notFound before acquiring any form owner',async()=>{
  owner.binding.mockRejectedValueOnce(Error('HOLDING_NOT_FOUND'));
  await expect(page()).rejects.toMatchObject(missing);
  expect(owner.queries).not.toHaveBeenCalled();expect(owner.read).not.toHaveBeenCalled();
 });
 it.each([
  Error('Original Holding infrastructure unavailable'),
  new ApiError(503,'CONTROL_UNAVAILABLE','HOLDING_NOT_FOUND'),
 ])('preserves an unexpected Holding lookup failure by identity (%s)',async failure=>{
  owner.binding.mockRejectedValueOnce(failure);
  await expect(page()).rejects.toBe(failure);
  expect(owner.queries).not.toHaveBeenCalled();expect(owner.read).not.toHaveBeenCalled();
 });
 it.each([
  Error('Original Holding resource unavailable'),
  new ApiError(503,'FORM_RESOURCE_UNAVAILABLE','Original form resource unavailable'),
 ])('preserves owner construction failures by identity without a reader fallback (%s)',async failure=>{
  owner.queries.mockRejectedValueOnce(failure);
  await expect(page()).rejects.toBe(failure);
  expect(owner.read).not.toHaveBeenCalled();
 });
 it.each([400,401,403,404,409,422,429])('hides an ordinary typed public owner refusal %i behind real Next notFound',async status=>{
  const failure=new ApiError(status,'PUBLIC_FORM_REFUSAL','Private publication or purchase detail');
  owner.read.mockRejectedValueOnce(failure);
  await expect(page()).rejects.toMatchObject(missing);
  expect(owner.binding).toHaveBeenCalledExactlyOnceWith(target.holdingSlug);
  expect(owner.queries).toHaveBeenCalledExactlyOnceWith('h001');
  expect(owner.read).toHaveBeenCalledExactlyOnceWith(target.formId);
 });
 it.each([
  Error('Original public form database failure'),
  Error('HOLDING_NOT_FOUND'),
  new ApiError(500,'PUBLIC_FORM_INTERNAL','Original form infrastructure failure'),
  new ApiError(503,'PUBLIC_FORM_UNAVAILABLE','Original form source temporarily unavailable'),
 ])('preserves unexpected or server-side reader failure by identity (%s)',async failure=>{
  owner.read.mockRejectedValueOnce(failure);
  await expect(page()).rejects.toBe(failure);
  expect(owner.read).toHaveBeenCalledExactlyOnceWith(target.formId);
 });
});
