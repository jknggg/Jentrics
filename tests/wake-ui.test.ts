import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { mountDashboard } from "../src/controller.ts";
import type { Bridge, DashboardState, WakeOffer, WakeOutcome } from "../src/types.ts";
import { previewBridge } from "../src/preview.ts";

function sample(): DashboardState {
    const now=Date.now();
    return {schemaVersion:1,agents:[{agentId:"codex",version:"0.165.0-alpha.1.2",availability:"detected",authState:"ready",capabilities:{discovery:true,authReadiness:true,quotas:true,usage:false,sessionStart:false},observedMs:now,diagnosticCode:null,quota:null}],refreshing:false,lastRefreshMs:now,storageError:null,collectionError:null};
}
function offer():WakeOffer {return {agentId:"codex",enabled:true,reason:null,modelLabel:"Provider default (subscription)",prompt:"Reply only with hello.",quotaUnverified:true,confirmationId:"one-use-id"};}
function outcome(status:WakeOutcome["status"]="completed",quotaEffect:WakeOutcome["quotaEffect"]="unverified"):WakeOutcome {return {agentId:"codex",status,reason:status==="timedOut"?"timedOut":null,quotaEffect,dashboard:sample()};}
function deferred<T>() {let resolve!:(value:T)=>void;const promise=new Promise<T>(yes=>{resolve=yes});return {promise,resolve};}
const flush=()=>new Promise(resolve=>setTimeout(resolve,0));
async function mount(overrides:Partial<Bridge>={}) {
    const {document}=parseHTML("<!doctype html><html><body><main id='root'></main></body></html>");
    Object.defineProperty(globalThis,"document",{value:document,configurable:true});
    let prepares=0;const confirmations:unknown[][]=[];
    const bridge:Bridge={preview:false,getState:async()=>sample(),refresh:async()=>sample(),subscribe:async()=>()=>{},prepareWake:async()=>{prepares++;return offer()},confirmWake:async(...args)=>{confirmations.push(args);return outcome()},...overrides};
    const root=document.querySelector("#root") as unknown as HTMLElement;
    const controller=await mountDashboard(root,bridge);
    return {document,root,controller,confirmations,prepares:()=>prepares,button:()=>root.querySelector<HTMLButtonElement>("#wake-codex")!};
}

test("wake cancellation sends no confirmed request and presents the fixed disclosure",async()=>{
    const app=await mount();try{
        assert.ok(app.button(),"manual wake control is present");app.button().click();await flush();
        const dialog=app.root.querySelector('[role="dialog"]');assert.ok(dialog);assert.match(dialog.textContent ?? "",/Reply only with hello\./);
        assert.match(dialog.textContent ?? "",/Provider default/);assert.match(dialog.textContent ?? "",/uses.*usage/i);assert.match(dialog.textContent ?? "",/no saved chat/i);assert.match(dialog.textContent ?? "",/cannot.*verify/i);
        app.root.querySelector<HTMLButtonElement>("#wake-cancel")!.click();await flush();
        assert.equal(app.confirmations.length,0);assert.equal(app.root.querySelector('[role="dialog"]'),null);
    }finally{app.controller.dispose();}
});
test("confirm and repeated clicks send exactly one request while busy",async()=>{
    const pending=deferred<WakeOutcome>();let calls=0;
    const app=await mount({confirmWake:async(agent,id,confirmed)=>{assert.deepEqual([agent,id,confirmed],["codex","one-use-id",true]);calls++;return pending.promise}});
    try{app.button().click();app.button().click();await flush();assert.equal(app.prepares(),1);
        const confirm=app.root.querySelector<HTMLButtonElement>("#wake-confirm")!;confirm.click();confirm.click();app.button().click();assert.equal(calls,1);assert.equal(app.button().disabled,true);
        pending.resolve(outcome("completed","unchanged"));await flush();assert.match(app.root.textContent ?? "",/Request completed/);assert.match(app.root.textContent ?? "",/unchanged/i);assert.doesNotMatch(app.root.textContent ?? "",/window started/i);
    }finally{pending.resolve(outcome());app.controller.dispose();}
});
test("fresh active five-hour and positively known weekly-only snapshots disable wake",async()=>{
    for(const minutes of [300,10080]){
        const state=sample();const now=Date.now();state.agents[0]!.quota={source:"codexAppServer",observedMs:now,diagnosticCode:null,windows:[{bucketId:"primary",windowMinutes:minutes,usedPercent:0,resetsAtMs:now+300000,observedMs:now}]};
        const app=await mount({getState:async()=>state});try{assert.equal(app.button().disabled,true);assert.match(app.root.textContent ?? "",minutes===300?/five-hour window is already active/i:/weekly-only/i);app.button().click();assert.equal(app.prepares(),0);}finally{app.controller.dispose();}
    }
});
test("unknown duration is not weekly-only and stale snapshots require fresh prepare",async()=>{
    for(const stale of [false,true]){
        const state=sample();const now=Date.now()-(stale?900001:0);state.agents[0]!.quota={source:"codexAppServer",observedMs:now,diagnosticCode:null,windows:[{bucketId:"primary",windowMinutes:stale?300:null,usedPercent:0,resetsAtMs:Date.now()+300000,observedMs:now}]};
        const app=await mount({getState:async()=>state});try{assert.equal(app.button().disabled,false);app.button().click();await flush();assert.equal(app.prepares(),1);assert.match(app.root.querySelector('[role="dialog"]')!.textContent ?? "",/cannot.*verify/i);}finally{app.controller.dispose();}
    }
});
test("rejected prepare and request errors show safe fixed messages",async()=>{
    const prepare=await mount({prepareWake:async()=>{throw new Error("SECRET raw provider output")}});try{prepare.button().click();await flush();assert.match(prepare.root.textContent ?? "",/could not check.*wake/i);assert.doesNotMatch(prepare.root.textContent ?? "",/SECRET/);}finally{prepare.controller.dispose();}
    const request=await mount({confirmWake:async()=>{throw new Error("SECRET raw output")}});try{request.button().click();await flush();request.root.querySelector<HTMLButtonElement>("#wake-confirm")!.click();await flush();assert.match(request.root.textContent ?? "",/completion.*unknown/i);assert.doesNotMatch(request.root.textContent ?? "",/SECRET/);}finally{request.controller.dispose();}
});
test("timeout reports unknown completion and observed-window text does not claim causation",async()=>{
    for(const result of [outcome("timedOut"),outcome("completed","newWindowObserved")]){
        const app=await mount({confirmWake:async()=>result});try{app.button().click();await flush();app.root.querySelector<HTMLButtonElement>("#wake-confirm")!.click();await flush();assert.match(app.root.textContent ?? "",result.status==="timedOut"?/Timed out.*completion unknown/i:/New five-hour window observed/);assert.doesNotMatch(app.root.textContent ?? "",/reset successful/i);}finally{app.controller.dispose();}
    }
});
test("disposal while preparing never opens confirmation or starts a request",async()=>{
    const pending=deferred<WakeOffer>();const app=await mount({prepareWake:()=>pending.promise});try{assert.ok(app.button());app.button().click();app.controller.dispose();pending.resolve(offer());await flush();assert.equal(app.root.querySelector('[role="dialog"]'),null);assert.equal(app.confirmations.length,0);}finally{app.controller.dispose();}
});
test("bridges without wake methods show an explicit unavailable action",async()=>{
    const app=await mount({prepareWake:undefined,confirmWake:undefined});try{assert.equal(app.button().disabled,true);assert.match(app.root.textContent ?? "",/Wake up is unavailable in this connection/);}finally{app.controller.dispose();}
});
test("only one confirmation can be prepared at a time across providers",async()=>{
    const pending=deferred<WakeOffer>();let calls=0;
    const app=await mount({prepareWake:()=>{calls++;return pending.promise}});
    try{app.button().click();app.root.querySelector<HTMLButtonElement>("#wake-claude")!.click();assert.equal(calls,1);pending.resolve(offer());await flush();assert.equal(app.root.querySelectorAll('[role="dialog"]').length,1);}finally{pending.resolve(offer());app.controller.dispose();}
});
test("backend eligibility refusal stays explicit without a confirmation or request",async()=>{
    const app=await mount({prepareWake:async()=>({...offer(),enabled:false,reason:"subscriptionRequired",confirmationId:null})});
    try{app.button().click();await flush();assert.equal(app.root.querySelector('[role="dialog"]'),null);assert.match(app.root.textContent ?? "",/first-party subscription/);assert.equal(app.confirmations.length,0);}finally{app.controller.dispose();}
});
test("synthetic preview has a cancellable simulated wake with no real-provider promise",async()=>{
    const bridge=previewBridge();assert.ok(bridge.prepareWake);assert.ok(bridge.confirmWake);
    const prepared=await bridge.prepareWake("claude");assert.equal(prepared.enabled,true);assert.equal(prepared.quotaUnverified,true);
    const result=await bridge.confirmWake("claude",prepared.confirmationId!,true);assert.equal(result.status,"completed");assert.equal(result.quotaEffect,"unverified");
    const app=await mount(bridge);try{app.root.querySelector<HTMLButtonElement>("#wake-claude")!.click();await flush();assert.match(app.root.querySelector('[role="dialog"]')!.textContent ?? "",/Synthetic preview only.*No real provider request/);}finally{app.controller.dispose();}
});
