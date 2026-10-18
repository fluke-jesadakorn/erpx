"use client";
import {createContext,useCallback,useContext,useEffect,useRef,useMemo,useState,type ReactNode} from "react";
import { usePathname } from "next/navigation";
import { useSearchParams } from "@/lib/shared/destination-search";
import {WorkspaceOverlayProvider} from "@/components/ui/workspace-overlay";
import styles from "./record-workspace.module.css";
type Host={node:HTMLDivElement;path:string};
const Context=createContext<{host:Host|null;register:(node:HTMLDivElement|null,path:string)=>void}>({host:null,register:()=>{}});
export function RecordWorkspaceProvider({children}:{children:ReactNode}){
 const path=usePathname(),[host,setHost]=useState<Host|null>(null);
 const register=useCallback((node:HTMLDivElement|null,path:string)=>setHost(previous=>node?{node,path}:previous?.path===path?null:previous),[]);
 const current=host?.path===path?host:null;
 const value=useMemo(()=>({host:current,register}),[current,register]);
 return <Context value={value}><WorkspaceOverlayProvider active={!!current}>{children}</WorkspaceOverlayProvider></Context>;
}
export function useRecordWorkspace(){return useContext(Context);}
export function WorkspaceRecordHost(){
 const {register}=useRecordWorkspace(),path=usePathname(),query=useSearchParams();
 const ref=useRef<HTMLDivElement>(null);
 useEffect(()=>{register(ref.current,path);return()=>register(null,path);},[register,path]);
 return <div ref={ref} className={styles.host} data-workspace-record-host data-open={!!query.get('detailKind')&&!!query.get('detailId')||undefined}/>;
}
