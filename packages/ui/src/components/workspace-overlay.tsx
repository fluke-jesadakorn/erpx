"use client";
import {createContext,useCallback,useContext,useEffect,useId,useMemo,useState,type ReactNode} from "react";
const Context=createContext<{active:boolean;editors:number;register:(id:string)=>()=>void}>({active:false,editors:0,register:()=>()=>{}});
export function WorkspaceOverlayProvider({active,children}:{active:boolean;children:ReactNode}) {
 const [ids,setIds]=useState<Set<string>>(()=>new Set());
 const register=useCallback((id:string)=>{setIds(previous=>new Set(previous).add(id));return ()=>setIds(previous=>{const next=new Set(previous);next.delete(id);return next;});},[]);
 const value=useMemo(()=>({active,editors:ids.size,register}),[active,ids,register]);
 return <Context value={value}>{children}</Context>;
}
export function useWorkspaceOverlay(){return useContext(Context);}
export function useWorkspaceEditor(open:boolean){const {active,register}=useWorkspaceOverlay(),id=useId();useEffect(()=>active&&open?register(id):undefined,[active,open,id,register]);}
