import "server-only";
import type {WorkContext} from "@/server/modules/identity_access/public/authorization";
import type {SqlDatabase} from "@/server/modules/runtime/public/runtime/types";
import {holdingResources} from "@/server/modules/runtime/public/runtime";
import {requirePermission} from "@/server/modules/identity_access/public/iam-permissions";
import {hasModuleAccess} from "@/data/module-access";
import {executeCommand,replayCommand} from "@/server/modules/workflow/public/commands";
import {ApiError} from "@/server/modules/runtime/public/http";
import {BusinessRuleError} from "../../shared";
import {createLearningCourseApplication, type LearningCourseInputPorts} from "../application/course";
export async function createLearningCourse(a:WorkContext,raw:unknown,key:string){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>resource??=(holdingResources(a.holdingId).then(x=>x.write));
 const ports:LearningCourseInputPorts={clock:{now:()=>Date.now()},
 async authorize(){requirePermission(a,"records.write");if(!hasModuleAccess(a.modules,"learning"))throw new ApiError(403,"MODULE_FORBIDDEN","Your current role cannot access Learning.");},
 async replay(key,operation,id,input){return replayCommand(await database(),a,key,operation,id,input);},
 async persist(command){const write=await database(),input=command.input,id=command.id,key=command.key,op=command.operation,now=command.now,result=command.result;
 return executeCommand(
    write,
    a,
    key,
    op,
    input,
    id,
    1,
    result,
    [
      write
        .prepare(
          "INSERT INTO learning_courses(id,company_id,title,description,provider,duration_minutes,required_for,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          a.companyId,
          input.title,
          input.description,
          input.provider,
          input.durationMinutes,
          input.requiredFor ?? null,
          a.userId,
          now,
          now,
        ),
      write
        .prepare(
          "INSERT INTO learning_course_events VALUES(?,?,1,'create',?,'Course drafted for independent review','draft',?)",
        )
        .bind(`learning-course-event-${key}`, id, a.userId, now),
    ],
  );

 }
 };try{return await createLearningCourseApplication(ports)(raw,key);}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}
}
