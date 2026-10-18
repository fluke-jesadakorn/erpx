"use client";
import {createContext,useContext,type ReactNode} from 'react';
import type {ProductHostKey} from '@/server/modules/commercial/public/rules/product-registry';
import type {ProductNavigationScope} from '@/lib/work_projection/product-navigation';
const Context=createContext<ProductNavigationScope>({audience:null});
export function ProductLinkEnvironment({audience,children}:{audience:ProductHostKey|null;children:ReactNode}){return <Context value={{audience}}>{children}</Context>;}
export function ProductWorkLinks({companyId,companySlug,workspaceBase,children}:{companyId:string;companySlug?:string;workspaceBase:string;children:ReactNode}){
 const parent=useContext(Context);
 return <Context value={{audience:parent.audience,companyId,companySlug,workspaceBase:workspaceBase.replace(/\/+$/,"")}}>{children}</Context>;
}
export function useProductNavigationScope(){return useContext(Context);}
