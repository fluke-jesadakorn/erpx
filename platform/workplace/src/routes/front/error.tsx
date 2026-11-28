"use client";
import { FrontError } from "@erpx/platform-front/components/front-error";
export default function FrontRouteError({error,reset}:{error:Error&{digest?:string};reset:()=>void}) {return <FrontError kind="unavailable" reference={error.digest} retry={reset}/>;}
