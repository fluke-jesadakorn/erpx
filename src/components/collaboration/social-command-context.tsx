"use client";
import {createContext,useContext,type ReactNode} from 'react';
import type {SocialCommandIdentity} from '@/lib/collaboration/social-command';
export type SocialRenderedContext=SocialCommandIdentity&{token:string;apiRoot:string};
const Context=createContext<SocialRenderedContext|null>(null);
export function SocialCommandProvider({value,children}:{value:SocialRenderedContext;children?:ReactNode}){return <Context.Provider value={value}>{children}</Context.Provider>;}
export function useSocialRenderedContext(){return useContext(Context);}
