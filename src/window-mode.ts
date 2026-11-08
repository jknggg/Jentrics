import {LogicalSize, PhysicalPosition, PhysicalSize} from "@tauri-apps/api/dpi";
import {Effect} from "@tauri-apps/api/window";
import type {Effects} from "@tauri-apps/api/window";
import type {DashboardWindowControls} from "./dashboard-presentation.ts";

export interface DashboardWindowPort {
    innerSize(): Promise<PhysicalSize>;
    outerPosition(): Promise<PhysicalPosition>;
    isMaximized(): Promise<boolean>;
    unmaximize(): Promise<void>;
    maximize(): Promise<void>;
    setSize(size: LogicalSize | PhysicalSize): Promise<void>;
    setPosition(position: PhysicalPosition): Promise<void>;
    setMinSize(size: LogicalSize): Promise<void>;
    setDecorations(value: boolean): Promise<void>;
    setAlwaysOnTop(value: boolean): Promise<void>;
    setShadow(value: boolean): Promise<void>;
    setBackgroundColor(color: string): Promise<void>;
    setEffects(effects: Effects): Promise<void>;
    clearEffects(): Promise<void>;
    close(): Promise<void>;
    startDragging(): Promise<void>;
}
interface Geometry {size: PhysicalSize; position: PhysicalPosition; maximized: boolean}

export function createWindowMode(window: DashboardWindowPort): DashboardWindowControls {
    let saved: Geometry | null = null;
    let compact = false;
    let pinned = false;
    async function applyMini() {
        await window.unmaximize();
        await window.setDecorations(false);
        await window.setShadow(false);
        await window.setMinSize(new LogicalSize(360, 430));
        await window.setSize(new LogicalSize(400, 480));
        await window.setBackgroundColor("#00000000");
        await window.setEffects({effects: [Effect.Acrylic], color: [18, 30, 49, 150]});
        await window.setAlwaysOnTop(pinned);
    }
    async function restore(geometry: Geometry) {
        await window.clearEffects();
        await window.setBackgroundColor("#ffffff");
        await window.setAlwaysOnTop(false);
        await window.setDecorations(true);
        await window.setShadow(true);
        await window.setMinSize(new LogicalSize(660, 500));
        await window.setSize(geometry.size);
        await window.setPosition(geometry.position);
        if (geometry.maximized) await window.maximize();
    }
    return {
        async setCompact(next) {
            if (compact === next) return;
            if (next) {
                const maximized = await window.isMaximized();
                // Capture restored bounds so expanding preserves both normal and maximized layouts.
                if (maximized) await window.unmaximize();
                try {
                    const [size, position] = await Promise.all([window.innerSize(), window.outerPosition()]);
                    saved = {size, position, maximized};
                    await applyMini(); compact = true;
                } catch (reason) {
                    try {if (saved) await restore(saved); else if (maximized) await window.maximize();}
                    catch (rollback) {throw new Error(`Mini view failed: ${String(reason)}. Window restoration also failed: ${String(rollback)}`);}
                    saved = null;
                    throw new Error(`Mini view could not open: ${String(reason)}`);
                }
            } else {
                if (!saved) throw new Error("Full dashboard window bounds are unavailable.");
                try {await restore(saved);}
                catch (reason) {
                    try {await applyMini();}
                    catch (rollback) {throw new Error(`Dashboard expansion failed: ${String(reason)}. Mini restoration also failed: ${String(rollback)}`);}
                    throw new Error(`Dashboard could not expand: ${String(reason)}`);
                }
                compact = false; pinned = false; saved = null;
            }
        },
        async setPinned(value) {await window.setAlwaysOnTop(value); pinned = value;},
        close: () => window.close(),
        drag: () => window.startDragging()
    };
}
