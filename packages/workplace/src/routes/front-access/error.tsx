"use client";
import { FrontError } from "@erpx/front/components/front-error";
export default function FrontAccessError({error,reset}:{error:Error&{digest?:string};reset:()=>void}) {return <FrontError kind="unavailable" reference={error.digest} retry={reset}/>;}
