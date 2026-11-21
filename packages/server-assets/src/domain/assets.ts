import type {Row} from '@erpx/server-commerce/public/commerce';
export type Asset=Row&{finance_asset_id:string|null;finance_asset_version:number|null;id:string;version:number;lifecycle_status:string;condition:string;placement:string|null;custodian_id:string|null;retirement_reason:string|null;created_by:string;updated_at:number};
