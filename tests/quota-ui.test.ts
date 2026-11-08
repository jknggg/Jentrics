import {test} from "node:test";
import assert from "node:assert/strict";
import {parseHTML} from "linkedom";
import {renderDashboard} from "../src/render.ts";
import type {DashboardState} from "../src/types.ts";

function sample() {
    const {document}=parseHTML("<html><body></body></html>");
    Object.defineProperty(globalThis,"document",{value:document,configurable:true});
    return {schemaVersion:1,agents:[{agentId:"codex",version:"0.162.1",availability:"detected",authState:"ready",capabilities:{discovery:true,authReadiness:true,quotas:true,usage:false,sessionStart:false},observedMs:1000,diagnosticCode:null,quota:{source:"codexAppServer",observedMs:1000,diagnosticCode:null,windows:[{bucketId:"codex:primary",windowMinutes:300,usedPercent:25,resetsAtMs:121000,observedMs:1000},{bucketId:"codex:secondary",windowMinutes:10080,usedPercent:null,resetsAtMs:null,observedMs:1000}]}}],refreshing:false,lastRefreshMs:1000,storageError:null,collectionError:null};
}
function view(state:ReturnType<typeof sample>){return renderDashboard(state as DashboardState);}
test("reported usage and unknown values remain distinct with source and resets",context=>{
    context.mock.timers.enable({apis:["Date"],now:1000});
    const node=view(sample());const text=node.textContent ?? "";
    assert.match(text,/25% used/);assert.match(text,/75% remaining/);assert.match(text,/Usage not reported/);assert.match(text,/Reset not reported/);assert.match(text,/Codex App Server/);assert.match(text,/Resets in 2m/);
    assert.equal(node.querySelectorAll("meter").length,1);
});
test("expired resets never imply replenishment and saved/aged observations are explicit",context=>{
    context.mock.timers.enable({apis:["Date"],now:181000});
    const state=sample();let text=view(state).textContent ?? "";
    assert.match(text,/Reset time passed/);assert.match(text,/25% used/);assert.doesNotMatch(text,/100% remaining/);
    state.lastRefreshMs=null as unknown as number;text=view(state).textContent ?? "";assert.match(text,/Saved quota snapshot/);
    state.lastRefreshMs=1000;context.mock.timers.tick(900000);assert.match(view(state).textContent ?? "",/Stale quota snapshot/);
});
test("quota errors and missing providers never render a successful quota",context=>{
    context.mock.timers.enable({apis:["Date"],now:1000});
    const state=sample();Object.assign(state.agents[0]!.quota,{diagnosticCode:"quotaUnavailable",windows:[]});
    const node=view(state);assert.match(node.textContent ?? "",/Quota check unavailable/);assert.equal(node.querySelectorAll("meter").length,0);assert.match(node.textContent ?? "",/Sign-in detected/);
    state.agents=[];assert.match(view(state).textContent ?? "",/Quota not checked/);
});
test("clock changes and failed refresh cannot present quota as current",context=>{
    context.mock.timers.enable({apis:["Date"],now:0});
    const state=sample();assert.match(view(state).textContent ?? "",/Clock changed/);
    context.mock.timers.tick(1000);Object.assign(state,{collectionError:"temporaryDirectoryUnavailable"});assert.match(view(state).textContent ?? "",/Quota unverified/);
    state.collectionError=null;assert.match(renderDashboard(state as DashboardState,{transportError:true}).textContent ?? "",/Quota unverified/);
});
test("Claude capture and Codex limits remain separate with truthful observation age",context=>{
    context.mock.timers.enable({apis:["Date"],now:1000000});
    const state=sample() as DashboardState;const codex=state.agents[0]!;
    state.agents.push({...codex,agentId:"claude",observedMs:1000000,quota:{source:"claudeStatusLine",observedMs:1000,diagnosticCode:null,windows:[{bucketId:"claude:five_hour",windowMinutes:300,usedPercent:12,resetsAtMs:500000,observedMs:1000}]}});state.lastRefreshMs=1000000;
    const node=renderDashboard(state);const claude=node.querySelector('[aria-labelledby="claude-quota-title"]')!;assert.ok(claude);assert.match(claude.textContent ?? "",/Claude Code usage limits/);assert.match(claude.textContent ?? "",/Claude status-line capture/);assert.match(claude.textContent ?? "",/Stale saved capture/);assert.match(claude.textContent ?? "",/12% used/);assert.match(claude.textContent ?? "",/Reset time passed/);assert.doesNotMatch(claude.textContent ?? "",/Reported just now|100% remaining/);assert.match(node.querySelector('[aria-labelledby="quota-title"]')!.textContent ?? "",/25% used/);
});
test("Claude capture missing fields remain unknown without a fabricated meter",()=>{
    const state=sample() as DashboardState;state.agents.push({...state.agents[0]!,agentId:"claude",quota:{source:"claudeStatusLine",observedMs:1000,diagnosticCode:null,windows:[{bucketId:"claude:seven_day",windowMinutes:10080,usedPercent:null,resetsAtMs:null,observedMs:1000}]}});
    const node=renderDashboard(state).querySelector('[aria-labelledby="claude-quota-title"]')!;assert.match(node.textContent ?? "",/Usage not reported/);assert.match(node.textContent ?? "",/Reset not reported/);assert.equal(node.querySelector("meter"),null);
});
