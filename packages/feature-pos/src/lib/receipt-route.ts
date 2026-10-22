import {recordDrawerHref} from '@erpx/feature-records/lib/record-drawer';
import {workspacePathParts} from '@erpx/feature-work_projection/lib/destination-registry';
export function posReceiptHref(current:string,id:string) {
 const url=new URL(current,'https://receipt.invalid');
 if(workspacePathParts(url.pathname))return recordDrawerHref(url.pathname+url.search,{kind:'pos_sale',id});
 const collection=url.pathname.replace(/\/receipts\/[^/]+\/overview$/,'').replace(/\/$/,'');
 const query=new URLSearchParams(url.search);
 for(const key of ['sale','detailKind','detailId','detailArgs','detailMode','returnTo','tab'])query.delete(key);
 query.set('returnTo',collection+(query.size?'?'+query:''));
 return `${collection}/receipts/${encodeURIComponent(id)}/overview?${query}`;
}
