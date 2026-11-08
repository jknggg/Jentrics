import { test } from "node:test";
import assert from "node:assert/strict";
import { parseHTML } from "linkedom";
import { renderDashboard } from "../src/render.ts";
import { mountDashboard } from "../src/controller.ts";
import type { DashboardState, Bridge } from "../src/types.ts";

function dom() {const {document}=parseHTML("<!doctype html><html><body><main id='root'></main></body></html>");Object.defineProperty(globalThis,"document",{value:document,configurable:true});return document;}
function sample(): DashboardState {return {schemaVersion:1,agents:[{agentId:"codex",version:"0.162.0-alpha.17.2",availability:"detected",authState:"ready",capabilities:{discovery:true,authReadiness:true,quotas:false,usage:false,sessionStart:false},observedMs:1000,diagnosticCode:null,quota:null}],refreshing:false,lastRefreshMs:1000,storageError:null,collectionError:null};}

test("missing agents and auth stay explicitly unchecked or unknown",()=>{
    dom();const state=sample();state.agents[0]!.version=null;state.agents[0]!.authState="unknown";
    const view=renderDashboard(state);assert.match(view.textContent ?? "",/Version unavailable/);assert.match(view.textContent ?? "",/Unknown/);assert.match(view.textContent ?? "",/Not checked/);
    assert.equal(view.querySelectorAll("tbody tr").length,2);
});
test("untrusted version text cannot create active markup",()=>{
    dom();const state=sample();state.agents[0]!.version='<img src=x onerror=alert(1)>';
    const view=renderDashboard(state);assert.equal(view.querySelectorAll("tbody img").length,0);assert.match(view.textContent ?? "",/<img src=x onerror=alert\(1\)>/);
});
test("unsupported and saved states cannot imply freshly verified authentication",()=>{
    dom();const state=sample();state.lastRefreshMs=null;state.agents[0]!.availability="unsupported";state.agents[0]!.authState="unknown";
    const view=renderDashboard(state);assert.match(view.textContent ?? "",/Unsupported/);assert.match(view.textContent ?? "",/Saved snapshot/);assert.doesNotMatch(view.textContent ?? "",/Sign-in detected/);
});
test("busy refresh and collection failures are visible",()=>{
    dom();const state=sample();state.refreshing=true;state.collectionError="temporaryDirectoryUnavailable";
    const view=renderDashboard(state);assert.ok(view.querySelector("button")?.hasAttribute("disabled"));assert.match(view.textContent ?? "",/Checking/);assert.match(view.textContent ?? "",/temporary directory/i);
});
test("preview is clearly synthetic and never presented as a live monitor",()=>{
    dom();const view=renderDashboard(sample(),{preview:true});assert.match(view.textContent ?? "",/Browser preview/);assert.match(view.textContent ?? "",/synthetic data/);
});
test("controller subscribes before reading and preserves newer events",async()=>{
    const document=dom();const order:string[]=[];let listener:((state:DashboardState)=>void)|undefined;let released=false;
    const fresh=sample();fresh.agents[0]!.version="0.162.1";
    const bridge:Bridge={preview:false,subscribe:async handler=>{order.push("subscribe");listener=handler;return ()=>{released=true}},getState:async()=>{order.push("read");listener!(fresh);return sample()},refresh:async()=>sample()};
    const mounted=await mountDashboard(document.querySelector("#root") as unknown as HTMLElement,bridge);
    assert.deepEqual(order,["subscribe","read"]);assert.match(document.body.textContent ?? "",/0.162.1/);mounted.dispose();assert.ok(released);
});
test("two clicks issue one refresh while the request is pending",async()=>{
    const document=dom();let requests=0;let resolveRefresh:((state:DashboardState)=>void)|undefined;
    const bridge:Bridge={preview:false,subscribe:async()=>()=>{},getState:async()=>sample(),refresh:()=>{requests++;return new Promise(resolve=>{resolveRefresh=resolve})}};
    const mounted=await mountDashboard(document.querySelector("#root") as unknown as HTMLElement,bridge);
    const first=document.querySelector("button")!;first.click();document.querySelector("button")!.click();
    assert.equal(requests,1);resolveRefresh!(sample());await new Promise(resolve=>setTimeout(resolve,0));assert.equal(document.querySelector("button")?.hasAttribute("disabled"),false);mounted.dispose();
});

test("an open dashboard ages snapshots without launching provider checks",async context=>{
    context.mock.timers.enable({apis:["Date","setInterval"],now:1000});
    const document=dom();let refreshCalls=0;
    const bridge:Bridge={preview:false,subscribe:async()=>()=>{},getState:async()=>sample(),refresh:async()=>{refreshCalls++;return sample()}};
    const mounted=await mountDashboard(document.querySelector("#root") as unknown as HTMLElement,bridge);
    assert.match(document.body.textContent ?? "",/Just now/);
    context.mock.timers.tick(60_000);assert.match(document.body.textContent ?? "",/1m ago/);
    context.mock.timers.tick(900_000);assert.match(document.body.textContent ?? "",/Stale snapshot/);assert.match(document.body.textContent ?? "",/Previously signed in/);
    assert.equal(refreshCalls,0);mounted.dispose();
});

test("desktop discloses tray refresh while synthetic preview does not claim background checks",()=>{
    dom();const native=renderDashboard(sample());assert.match(native.textContent ?? "",/every 5 minutes/);
    const preview=renderDashboard(sample(),{preview:true});assert.doesNotMatch(preview.textContent ?? "",/every 5 minutes/);assert.match(preview.textContent ?? "",/synthetic data/);
});
