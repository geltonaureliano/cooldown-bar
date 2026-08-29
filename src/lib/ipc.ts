import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Bootstrap, HoverState, MotionState } from "./types";

/** Initial paint data. */
export function getUsage(): Promise<Bootstrap> {
  return invoke<Bootstrap>("get_usage");
}

/**
 * Let the window accept clicks outside the bar while the menu is open.
 * Without it the menu's buttons land in the click-through region and do nothing.
 */
export function setMenuOpen(open: boolean): Promise<void> {
  return invoke("set_menu_open", { open });
}

export function quitApp(): Promise<void> {
  return invoke("quit");
}

export function reloadConfig(): Promise<void> {
  return invoke("reload");
}

export function requestRefresh(): Promise<void> {
  return invoke("refresh");
}

export function onState(handler: (state: Bootstrap) => void): Promise<UnlistenFn> {
  return listen<Bootstrap>("state://update", (e) => handler(e.payload));
}
export function onMenuClose(handler: () => void): Promise<UnlistenFn> {
  return listen("menu://close", handler);
}

/**
 * Hover comes from Rust, not CSS `:hover`.
 *
 * The panel toggles `ignoresMouseEvents` as the cursor crosses the bar edge, and
 * that toggle can swallow the `mouseenter` a CSS hover would need.
 */
export function onHover(handler: (state: HoverState) => void): Promise<UnlistenFn> {
  return listen<HoverState>("hover://update", (e) => handler(e.payload));
}

export function onMotion(handler: (state: MotionState) => void): Promise<UnlistenFn> {
  return listen<MotionState>("motion://update", (e) => handler(e.payload));
}
export function dockNearest(): Promise<void> { return invoke("dock_nearest"); }
export function setMotionPreferences(reduced: boolean): Promise<void> {
  return invoke("motion_preferences", { reduced });
}
