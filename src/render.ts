import { renderQuota } from "./quota.ts";
import type {DisplaySettings} from "./configuration-types.ts";
import {renderAccountUsage} from "./account-usage.ts";
import type { AgentState, DashboardState, DiagnosticCode } from "./types.ts";
import { renderWakeAction } from "./wake-ui.ts";
import type { WakeViewState } from "./wake-ui.ts";
export interface RenderOptions { display?:DisplaySettings|null; preview?: boolean; transportError?: boolean; wake?:WakeViewState }

const diagnostics: Record<DiagnosticCode,string> = {
    invalidHistoryRequest:"The history request is invalid.",historyConfirmationRequired:"Confirm before clearing history.",
    executableUnavailable:"A supported native executable is unavailable.",versionUnavailable:"The installed version could not be checked.",unsupportedVersion:"This version has not been verified. Readiness checks are disabled.",authUnavailable:"Sign-in status could not be checked.",probeTimeout:"The readiness check timed out.",probeOutputLimit:"The readiness response exceeded its size limit.",storageUnavailable:"Local history is unavailable. Current checks still work.",storageIncompatible:"This history was created by a newer version. It has been preserved.",storageCorrupt:"Local history could not be read. It has been preserved.",quotaUnavailable:"Quota check unavailable. Try Refresh now.",temporaryDirectoryUnavailable:"A temporary directory could not be created. Try Refresh now."
};
function element<K extends keyof HTMLElementTagNameMap>(tag:K,text="",className=""):HTMLElementTagNameMap[K] {
    const node=document.createElement(tag);node.textContent=text;if(className)node.className=className;return node;
}
function diagnostic(code:DiagnosticCode):string {return diagnostics[code] ?? "The readiness check is unavailable.";}
function checked(agent:AgentState|undefined,state:DashboardState):string {
    if(!agent || agent.observedMs<=0)return "Not checked";
    if(state.lastRefreshMs===null)return "Saved snapshot";
    const age=Date.now()-agent.observedMs;
    if(age<0)return "Clock changed";
    if(age<60_000)return "Just now";
    if(age<900_000)return `${Math.floor(age/60_000)}m ago`;
    return "Stale snapshot";
}
export function renderDashboard(state: DashboardState, options: RenderOptions = {}): HTMLElement {
    const view=element("div","","dashboard");
    const header=element("header","","app-header");
    const brand=element("div","","brand");const mark=element("span","J","brand-mark");mark.setAttribute("aria-hidden","true");
    brand.append(mark,element("span","Jentrics"));header.append(brand,element("span","Local on this device","local-label"));view.append(header);
    if(options.preview){const notice=element("p","Browser preview · synthetic data. This preview does not check installed agents.","preview-banner");view.append(notice);}
    const intro=element("section","","intro");const heading=element("div");heading.append(element("h1","Agent overview"),element("p","Check agent readiness and reported usage limits.","intro-copy"));
    const button=element("button",state.refreshing?"Checking…":"Refresh now","refresh-button");button.id="refresh";button.type="button";button.disabled=state.refreshing;intro.append(heading,button);view.append(intro);
    const section=element("section","","agents-section");const sectionHeader=element("div","","section-header");const title=element("h2","Installed agents");title.id="agents-title";
    const status=element("p",state.refreshing?"Checking installed agents…":state.lastRefreshMs!==null?"Readiness check finished":state.agents.length?"Saved snapshots · Refresh to verify":"Not checked yet","status-line");status.setAttribute("role","status");status.setAttribute("aria-live","polite");sectionHeader.append(title,status);section.append(sectionHeader);
    if(options.transportError){section.append(element("p","The local monitor is unavailable. Try Refresh now.","error-notice"));}
    for(const code of [state.storageError,state.collectionError])if(code){const notice=element("p",diagnostic(code),"error-notice");notice.setAttribute("role","status");section.append(notice);}
    const table=element("table","","agents-table");table.setAttribute("aria-labelledby","agents-title");const head=element("thead");const labels=element("tr");
    for(const label of ["Agent","Installation","Authentication","Checked"]){const th=element("th",label);th.scope="col";labels.append(th);}head.append(labels);table.append(head);
    const body=element("tbody");
    for(const [id,name] of [["codex","OpenAI Codex"],["claude","Claude Code"]] as const){
        const agent=state.agents.find(a=>a.agentId===id);const row=element("tr");const provider=element("th",name,"agent-name");provider.scope="row";provider.append(renderWakeAction(id,state,options.wake,options.transportError));row.append(provider);
        const installation=element("td","","installation");installation.dataset.label="Installation";
        const availability=agent?({notFound:"Not found",detected:"Detected",unsupported:"Unsupported",probeFailed:"Check unavailable"} as const)[agent.availability]:"Not checked";
        const availabilityLabel=element("span","",`availability ${agent?.availability ?? "unchecked"}`);const dot=element("span","","status-dot");dot.setAttribute("aria-hidden","true");availabilityLabel.append(dot,document.createTextNode(availability));
        installation.append(availabilityLabel,element("span",agent?.version ?? "Version unavailable","version"));if(agent?.diagnosticCode)installation.append(element("span",diagnostic(agent.diagnosticCode),"agent-diagnostic"));row.append(installation);
        const authentication=element("td","","authentication");authentication.dataset.label="Authentication";
        const stale=agent && (state.lastRefreshMs===null || Date.now()-agent.observedMs>900_000 || agent.observedMs>Date.now());
        authentication.textContent=agent?.authState==="ready"?(stale?"Previously signed in":"Sign-in detected"):agent?.authState==="notAuthenticated"?(stale?"Previously signed out":"Not signed in"):"Unknown";row.append(authentication);
        const time=element("td",checked(agent,state),"checked");time.dataset.label="Checked";if(agent?.observedMs)time.title=new Date(agent.observedMs).toLocaleString();row.append(time);body.append(row);
    }
    table.append(body);section.append(table);view.append(section,renderQuota(state,options.transportError,options.display),renderAccountUsage(state));
    const note=element("aside","","privacy-note");note.append(element("h2","Monitoring stays local"),element("p",options.preview?"Refresh checks readiness and reported usage limits. Preview wake actions are synthetic and send no model requests.":"Automatic checks follow your monitoring settings (default: every 5 minutes) while Jentrics is running, including in the tray. Automatic checks send no model requests. Wake up is disabled under the retry/isolation policy."));view.append(note);
    const footer=element("footer","","app-footer");footer.append(element("span","Quota alpha"),element("span","Close this window to return to the tray."));view.append(footer);
    return view;
}
