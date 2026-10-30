// Qualification only: real native HTTP commands with synthetic Account proof.
import fs from 'node:fs';
import http from 'node:http';
import {randomUUID} from 'node:crypto';
const role=process.argv[2];if(!['finance','maker'].includes(role))throw Error('PERSONA_INVALID');
const dir='/case/',proof=JSON.parse(fs.readFileSync(`${dir}account-proof-${role}.json`,'utf8')),company=proof.companyId;
function request(path,{body,method='GET'}={}) {return new Promise((resolve,reject)=>{
 const headers={host:'acme.jesadakorn.com',cookie:proof.cookie,accept:'application/json','x-erpx-context-company':company,'x-erpx-context-assignment':proof.assignmentId,'x-erpx-context-revision':String(proof.revision),'x-erpx-context-assignment-version':String(proof.assignmentVersion),...(body?{'content-type':'application/json','idempotency-key':randomUUID(),origin:'https://acme.jesadakorn.com'}:{})};
 const req=http.request({hostname:'127.0.0.1',port:44817,path:'/api/v1/holdings/acme/field-service/'+path,method:body?'POST':method,headers},res=>{const chunks=[];res.on('data',x=>chunks.push(x));res.on('end',()=>{try{const data=JSON.parse(Buffer.concat(chunks));if(!(body?[200,201].includes(res.statusCode):res.statusCode===200))return reject(Error(`NATIVE_${path.split('?')[0]}_${res.statusCode}_${data?.error?.code??'UNKNOWN'}`));if(data.context?.companyId!==company||data.context?.assignmentId!==proof.assignmentId||data.context?.assignmentVersion!==proof.assignmentVersion||data.context?.revision!==proof.revision)return reject(Error('NATIVE_SELECTED_CONTEXT_MISMATCH'));resolve(data.data);}catch(error){reject(error);}});});req.on('error',reject);req.end(body?JSON.stringify(body):undefined);
});}
const envelope=async()=>({expectedVersion:0,contextKey:(await request('parts-setup/prerequisites')).contextKey});
const post=async(path,input)=>request('parts-setup/'+path,{body:{...await envelope(),input}});
if(role==='finance'){
 const initial=await request('parts-setup/prerequisites'),initialMapping=await request('parts-setup');
 if(initial.available!==true||initial.charts.length!==0||initial.accounts.length!==0||initial.periods.length!==0||initial.rates.length!==0||initialMapping.configured!==false||initialMapping.sourceReady!==false)throw Error('PARTS_INITIAL_STATE_INVALID');
 const chart=await post('charts',{name:'Container synthetic chart',baseCurrency:'THB',accountingBasis:'IFRS'}),roles=[['receivableAccountId','1100','Receivable','asset','receivable'],['payableAccountId','2100','Payable','liability','payable'],['revenueAccountId','4100','Revenue','revenue','none'],['inventoryAccountId','1200','Inventory','asset','none'],['clearingAccountId','2200','Clearing','liability','none'],['cogsAccountId','5100','Cost of goods','expense','none'],['inputTaxAccountId','1300','Input tax','asset','none'],['outputTaxAccountId','2300','Output tax','liability','none'],['cashAccountId','1000','Cash','asset','bank'],['variance','5400','Variance','expense','none']],ids={};
 for(const [field,code,name,category,controlKind] of roles){const account=await post('accounts',{chartId:chart.id,code,name,category,currency:'THB',controlKind,isControl:controlKind!=='none'});ids[field]=account.id;}
 await post('periods',{name:'Container open period',startDate:'2026-01-01',endDate:'2026-12-31'});
 await post('rates',{currency:'THB',effectiveDate:'2026-01-01',currencyPerUsd:'35',note:''});
 const mapping=await request('parts-setup');const configured=await request('parts-setup',{body:{expectedVersion:0,contextKey:mapping.contextKey,chartId:chart.id,currency:'THB',reason:'Synthetic reviewed Service setup',...Object.fromEntries(roles.slice(0,9).map(([field])=>[field,ids[field]])),...(mapping.expectedSalesDefaults===undefined?{}:{expectedSalesDefaults:mapping.expectedSalesDefaults})}});
 const readback=await request('parts-setup');if(configured.id!==company||configured.status!=='configured'||configured.version!==1||readback.configured!==true||readback.sourceReady!==true||readback.configuration?.chartId!==chart.id)throw Error('PARTS_MAPPING_READBACK_INVALID');
 fs.writeFileSync(dir+'finance-setup.json',JSON.stringify({chartId:chart.id,initialReferencesEmpty:true,mappingConfigured:true}),{mode:0o600,flag:'wx'});console.log('native Finance and Service setup commands passed');
}else{
 const warehouse=await request('parts-setup/masters/warehouses',{body:{code:'C-WH',name:'Container warehouse',branchId:'branch',status:'active'}});
 const product=await request('parts-setup/masters/products',{body:{sku:'C-PART',name:'Container part',unit:'each',itemType:'stock',trackingMode:'none',price:'12.34',taxBps:0,status:'active'}});
 if(!warehouse.id||warehouse.version!==1||!product.id||product.version!==1)throw Error('PARTS_MASTER_RECEIPT_INVALID');
 const query=new URLSearchParams({warehouseId:warehouse.id,productId:product.id,countDate:'2026-10-01'});const selected=await request('initial-physical-stock?'+query);
 if(!selected.canPrepare||!selected.contextKey)throw Error('COUNT_SELECTION_NOT_PREPARABLE');
 const count=await request('initial-physical-stock',{body:{expectedVersion:0,contextKey:selected.contextKey,input:{warehouseId:warehouse.id,countDate:'2026-10-01',reason:'Container synthetic Count',lines:[{productId:product.id,countedQuantity:'2',unitCost:'3',allocations:[]}]}}});
 const detail=await request('initial-physical-stock/'+encodeURIComponent(count.id));if(count.status!=='draft'||count.version!==1||detail.record?.id!==count.id)throw Error('COUNT_CREATE_READBACK_INVALID');
 fs.writeFileSync(dir+'count-result.json',JSON.stringify({id:count.id,version:count.version,companyId:company,warehouseId:warehouse.id,productId:product.id}),{mode:0o600,flag:'wx'});console.log('native Count create and selected readback passed');
}
