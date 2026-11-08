import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
const config=JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json",import.meta.url),"utf8"));
const capability=JSON.parse(readFileSync(new URL("../src-tauri/capabilities/dashboard.json",import.meta.url),"utf8"));
test("desktop starts without windows and IPC belongs only to the local dashboard",()=>{
    assert.deepEqual(config.app.windows,[]);assert.equal(config.app.withGlobalTauri,false);assert.deepEqual(config.app.security.capabilities,["dashboard"]);
    assert.deepEqual(capability.windows,["dashboard"]);assert.equal(capability.local,true);assert.equal(capability.remote,undefined);
    assert.deepEqual(new Set(capability.permissions),new Set([
        "core:event:allow-listen","core:event:allow-unlisten","allow-get-dashboard-state",
        "core:window:allow-inner-size","core:window:allow-outer-position","core:window:allow-is-maximized",
        "core:window:allow-unmaximize","core:window:allow-maximize","core:window:allow-set-size",
        "core:window:allow-set-position","core:window:allow-set-min-size","core:window:allow-set-decorations",
        "core:window:allow-set-shadow","core:window:allow-set-effects","core:window:allow-set-always-on-top",
        "core:window:allow-set-background-color","core:webview:allow-set-webview-background-color",
        "core:window:allow-close","core:window:allow-start-dragging",
        "allow-export-configuration","allow-import-configuration","allow-get-display-settings",
        "allow-set-display-settings","allow-get-storage-maintenance","allow-get-adapter-catalog",
        "allow-get-diagnostics-status","allow-take-settings-request",
        "allow-refresh-agents-command","allow-prepare-wake","allow-confirm-wake",
        "allow-get-history","allow-get-history-settings","allow-set-history-retention","allow-clear-history",
        "allow-get-claude-bridge-status","allow-prepare-claude-bridge","allow-apply-claude-bridge","allow-restore-claude-bridge",
        "allow-get-alert-settings","allow-set-alert-settings","allow-request-alert-permission","allow-get-metrics",
        "allow-set-metric-settings","allow-clear-metric-history","allow-get-pricing","allow-set-price-override",
        "allow-remove-price-override","allow-estimate-cost","allow-get-monitor-settings","allow-set-monitor-settings",
        "allow-send-test-notification"
    ]));
    assert.match(config.app.security.csp,/object-src 'none'/);assert.doesNotMatch(config.app.security.csp,/unsafe-eval|unsafe-inline|https?:\/\/\*/);
});
