import {test} from "node:test";
import assert from "node:assert/strict";
import {LogicalSize, PhysicalPosition, PhysicalSize} from "@tauri-apps/api/dpi";
import {createWindowMode} from "../src/window-mode.ts";
import type {DashboardWindowPort} from "../src/window-mode.ts";

function fakeWindow() {
    const state = {size: new PhysicalSize(1000, 800), position: new PhysicalPosition(170, 90), maximized: true,
        decorated: true, pinned: false, glass: false, shadow: true, background: "#ffffff", minimum: new LogicalSize(660, 500), closed: false, dragged: false};
    const port: DashboardWindowPort = {
        innerSize: async () => state.size, outerPosition: async () => state.position, isMaximized: async () => state.maximized,
        unmaximize: async () => {state.maximized = false;}, maximize: async () => {state.maximized = true;},
        setSize: async size => {state.size = new PhysicalSize(size.width, size.height);},
        setPosition: async position => {state.position = position;},
        setMinSize: async size => {state.minimum = size;}, setDecorations: async value => {state.decorated = value;},
        setAlwaysOnTop: async value => {state.pinned = value;}, setShadow: async value => {state.shadow = value;},
        setEffects: async () => {state.glass = true;}, clearEffects: async () => {state.glass = false;},
        setBackgroundColor: async value => {state.background = value;},
        close: async () => {state.closed = true;}, startDragging: async () => {state.dragged = true;}
    };
    return {state, port};
}
test("native compact transition restores full geometry, chrome and maximization, clearing pin", async () => {
    const {state, port} = fakeWindow(); const controls = createWindowMode(port);
    await controls.setCompact(true);
    assert.deepEqual([state.size.width, state.size.height], [400, 480]);
    assert.equal(state.decorated, false); assert.equal(state.glass, true); assert.equal(state.maximized, false);
    assert.equal(state.background, "#00000000");
    await controls.setPinned(true); await controls.drag();
    state.position = new PhysicalPosition(500, 300);
    await controls.setCompact(false);
    assert.deepEqual([state.size.width, state.size.height], [1000, 800]);
    assert.deepEqual([state.position.x, state.position.y], [170, 90]);
    assert.equal(state.maximized, true); assert.equal(state.decorated, true); assert.equal(state.glass, false); assert.equal(state.pinned, false);
    assert.equal(state.background, "#ffffff");
    assert.equal(state.minimum.width, 660); assert.equal(state.dragged, true);
    await controls.close(); assert.equal(state.closed, true);
});
test("native acrylic failure restores dashboard and propagates a visible failure", async () => {
    const {state, port} = fakeWindow(); port.setEffects = async () => {throw Error("Acrylic unavailable");};
    const controls = createWindowMode(port);
    await assert.rejects(controls.setCompact(true), /Acrylic unavailable/);
    assert.equal(state.size.width, 1000); assert.equal(state.decorated, true); assert.equal(state.maximized, true);
    assert.equal(state.background, "#ffffff");
});
test("failed expansion returns to compact mode and allows retry", async () => {
    const {state, port} = fakeWindow(); const controls = createWindowMode(port); await controls.setCompact(true);
    const original = port.setDecorations; let fail = true;
    port.setDecorations = async value => {if(value && fail){fail = false; throw Error("Chrome unavailable");} await original(value);};
    await assert.rejects(controls.setCompact(false), /Chrome unavailable/);
    assert.equal(state.size.width, 400); assert.equal(state.decorated, false);
    assert.equal(state.background, "#00000000");
    await controls.setCompact(false); assert.equal(state.size.width, 1000); assert.equal(state.decorated, true);
});
