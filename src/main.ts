import {isTauri} from "@tauri-apps/api/core";
import {mountDashboard} from "./controller.ts";
import {nativeBridge} from "./native-bridge.ts";
import {previewBridge} from "./preview.ts";
import "./styles.css";
import "./mini-dashboard.css";
import {getCurrentWebviewWindow} from "@tauri-apps/api/webviewWindow";
import {createWindowMode} from "./window-mode.ts";

const bridge=isTauri()?nativeBridge():previewBridge();
const root=document.querySelector<HTMLElement>("#app")!;
void mountDashboard(root,bridge,isTauri()?createWindowMode(getCurrentWebviewWindow()):undefined).then(controller=>window.addEventListener("pagehide",()=>controller.dispose(),{once:true}));
