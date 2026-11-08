import type {AgentId,DashboardState, QuotaWindow} from "./types.ts";
import type {DisplaySettings} from "./configuration-types.ts";
import {renderQuotaCountdown} from "./countdown.ts";

function node<K extends keyof HTMLElementTagNameMap>(tag:K,text="",className=""):HTMLElementTagNameMap[K] {
    const result=document.createElement(tag);result.textContent=text;
    if(className)result.className=className;
    return result;
}
function duration(minutes:number):string {
    if(minutes%1440===0)return `${minutes/1440}-day window`;
    if(minutes%60===0)return `${minutes/60}-hour window`;
    return `${minutes}-minute window`;
}
function renderProviderQuota(state:DashboardState,id:AgentId,transportError:boolean,display?:DisplaySettings|null):HTMLElement {
    const titleId=id==="codex"?"quota-title":"claude-quota-title";const provider=id==="codex"?"Codex":"Claude Code";
    const section=node("section","","quota-section");section.setAttribute("aria-labelledby",titleId);
    const heading=node("div","","section-header");const title=node("h2",`${provider} usage limits`);title.id=titleId;heading.append(title);section.append(heading);
    const agent=state.agents.find(agent=>agent.agentId===id);const quota=agent?.quota;
    const primary=display?.primaryBuckets[id];
    if(primary!==undefined && primary!==null && !quota?.windows.some(window=>window.bucketId===primary))section.append(node("p",`Selected primary bucket unavailable: ${primary}. Other reported buckets remain visible.`,"error-notice"));
    if(!quota){
        const reason=id==="claude"?"No captured Claude usage limits yet. Optional usage capture requires your review and confirmation.":agent?.availability==="unsupported"?"Quota unavailable for this Codex version.":agent?.availability==="notFound"?"Install a supported native Codex executable to check usage limits.":"Quota not checked · Refresh to check supported Codex installations.";
        section.append(node("p",reason,"quota-meta"));return section;
    }
    const now=Date.now();const age=now-quota.observedMs;
    const captured=quota.source==="claudeStatusLine";
    const clockChanged=age<0;
    const freshness=transportError?"Quota unverified · Local monitor unavailable":state.collectionError?"Quota unverified · Last refresh failed":state.lastRefreshMs===null?(captured?"Saved capture · Freshness unverified":"Saved quota snapshot · Refresh to verify"):clockChanged?(captured?"Capture clock changed · Freshness unknown":"Clock changed · Refresh to verify"):age>=900000?(captured?"Stale saved capture · Await a new Claude capture":"Stale quota snapshot · Refresh to verify"):`Reported ${age<60000?"just now":`${Math.floor(age/60000)}m ago`}`;
    heading.append(node("p",freshness,"status-line"));
    const source=node("p",captured?"Source: Claude status-line capture · Last saved reported observation. Refresh reads this capture without contacting Claude.":"Source: Codex App Server · Last reported snapshot","quota-meta");source.title=new Date(quota.observedMs).toLocaleString();section.append(source);
    if(quota.diagnosticCode){
        const detail=quota.diagnosticCode==="probeTimeout"?"The quota request timed out.":quota.diagnosticCode==="probeOutputLimit"?"The quota response exceeded its size limit.":"Usage limits could not be read.";
        section.append(node("p",`Quota check unavailable · ${detail} ${captured?"Await a valid Claude capture.":"Try Refresh now."}`,"error-notice"));return section;
    }
    if(!quota.windows.length){section.append(node("p","Usage limits not reported for this account.","quota-meta"));return section;}
    const cards=node("div","","quota-grid");
    for(const window of quota.windows){
        const card=node("article","","quota-card");
        if(primary===window.bucketId){card.classList.add("quota-primary");card.append(node("p","Primary quota window","quota-meta"));}
        const label=window.windowMinutes===null?"Window duration not reported":duration(window.windowMinutes);
        card.append(node("h3",label),node("p",window.bucketId,"quota-bucket"));
        if(window.usedPercent===null)card.append(node("p","Usage not reported","quota-value"));
        else {
            const used=new Intl.NumberFormat(undefined,{maximumFractionDigits:2}).format(window.usedPercent);
            const remaining=new Intl.NumberFormat(undefined,{maximumFractionDigits:2}).format(100-window.usedPercent);
            card.append(node("p",`${used}% used`,"quota-value"),node("p",`${remaining}% remaining`,"quota-meta"));
            const meter=node("meter");meter.min=0;meter.max=100;meter.value=window.usedPercent;
            meter.setAttribute("aria-label",`${window.bucketId}, ${label}, usage`);card.append(meter);
        }
        const reset=node("p","","quota-reset");reset.append(renderQuotaCountdown(window.resetsAtMs,window.observedMs,captured));
        if(window.resetsAtMs!==null){const time=node("time",new Date(window.resetsAtMs).toLocaleString());time.dateTime=new Date(window.resetsAtMs).toISOString();reset.append(node("br"),time);}
        card.append(reset);cards.append(card);
    }
    section.append(cards);return section;
}
export function renderQuota(state:DashboardState,transportError=false,display?:DisplaySettings|null):HTMLElement {
    const sections=node("div","","quota-sections");
    for(const id of ["codex","claude"] as const)sections.append(renderProviderQuota(state,id,transportError,display));
    return sections;
}
