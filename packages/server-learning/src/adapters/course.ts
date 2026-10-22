import "server-only";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {holdingResources} from "../../../../packages/server-runtime/src/public/runtime";
import {requirePermission} from "@erpx/server-identity_access/public/iam-permissions";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {ApiError} from "@erpx/server-runtime/public/http";
import {BusinessRuleError} from "@erpx/server-shared/index";
import {createLearningCourseApplication, type LearningCourseInputPorts} from "@erpx/server-learning/application/course";
import {withLearningPurchasedCommand} from "@erpx/server-learning/adapters/purchases";
export async function createLearningCourse(a:WorkContext,raw:unknown,key:string){
 const owner=(await holdingResources(a.holdingId)).write;
 return withLearningPurchasedCommand(owner,a,{key,operation:"learning.course.create",target:{kind:"course",id:`learning-course-${key}`},purpose:"new-work",capabilities:["learn.courses"]},write=>nativeCreateLearningCourse(write,a,raw,key));
}
async function nativeCreateLearningCourse(write:SqlDatabase,a:WorkContext,raw:unknown,key:string){
 const database=async()=>write;
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
