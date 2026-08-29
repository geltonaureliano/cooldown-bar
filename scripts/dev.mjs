#!/usr/bin/env node
// Own one development process group. Never kill by port or broad process name.
import { spawn, spawnSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { readFileSync, writeFileSync, unlinkSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(import.meta.url);
const root = resolve(dirname(script), "..");
const stateFile = resolve(root, ".cooldown-bar-dev.json");
export function isOwned(record, command) {
  return Number.isSafeInteger(record?.pid) && record.pid > 1 && record.script === script
    && typeof record.token === "string" && /^[a-f0-9-]{36}$/.test(record.token)
    && command.includes(`${script} worker ${record.token}`);
}
function record() {
  try { return JSON.parse(readFileSync(stateFile, "utf8")); } catch { return null; }
}
function commandOf(pid) {
  if (!Number.isSafeInteger(pid) || pid < 2) return "";
  return spawnSync("/bin/ps", ["-p", String(pid), "-o", "command="], { encoding: "utf8" }).stdout?.trim() ?? "";
}
function owned() { const r = record(); return isOwned(r, commandOf(r?.pid)) ? r : null; }
function signal(pid, value) { try { process.kill(-pid, value); } catch (e) { if (e.code !== "ESRCH") throw e; } }
function clean(token) { if (record()?.token === token) { try { unlinkSync(stateFile); } catch {} } }

async function main(action, token) {
  if (action === "stop") {
    const r = owned();
    if (!r) { console.log("No managed Cooldown Bar development process. Other processes were left alone."); return; }
    signal(r.pid, "SIGTERM");
    const deadline = Date.now() + 6000;
    while (owned() && Date.now() < deadline) await new Promise((resolve) => setTimeout(resolve, 100));
    if (owned()) throw new Error("The managed process has not stopped yet; no unrelated process was touched.");
    console.log("This project's development process stopped.");
    return;
  }
  if (action === "worker") {
    if (!/^[a-f0-9-]{36}$/.test(token ?? "")) throw new Error("Invalid worker token");
    // Exclusive creation also prevents two simultaneous `make dev` commands.
    writeFileSync(stateFile, JSON.stringify({ pid: process.pid, token, script }), { flag: "wx", mode: 0o600 });
    let stopping = false;
    const stop = () => {
      if (stopping) return;
      stopping = true;
      signal(process.pid, "SIGTERM");
      setTimeout(() => { clean(token); signal(process.pid, "SIGKILL"); }, 4000).unref();
    };
    process.on("SIGINT", stop);
    process.on("SIGTERM", stop);
    const child = spawn("npm", ["run", "tauri", "dev"], { cwd: root, stdio: "inherit" });
    child.on("error", (error) => { console.error(error.message); stop(); });
    child.on("exit", (code) => {
      process.send?.({ exitCode: stopping ? 0 : (code ?? 1) });
      stop();
      // Allow the native app's signal handler to stop provider subprocesses.
      setTimeout(() => { clean(token); signal(process.pid, "SIGKILL"); }, 1500);
    });
    return;
  }
  if (action !== "start") throw new Error("Usage: node scripts/dev.mjs start|stop");
  if (owned()) throw new Error("This project's dev process is already running. Use make stop first.");
  if (existsSync(stateFile)) {
    const stale = record();
    if (!stale || commandOf(stale.pid)) throw new Error("Unverified dev state; refusing to overwrite it or signal any process.");
    unlinkSync(stateFile);
  }
  const child = spawn(process.execPath, [script, "worker", randomUUID()], { cwd: root, detached: true, stdio: ["inherit", "inherit", "inherit", "ipc"] });
  let workerResult;
  child.on("message", (message) => { if (Number.isInteger(message?.exitCode)) workerResult = message.exitCode; });
  for (const event of ["SIGINT", "SIGTERM"]) process.on(event, () => signal(child.pid, "SIGTERM"));
  child.on("error", (error) => { console.error(error.message); process.exitCode = 1; });
  child.on("exit", (code, received) => { process.exitCode = workerResult ?? (received === "SIGKILL" ? 0 : (code ?? 1)); });
}
if (process.argv[1] && resolve(process.argv[1]) === script) {
  main(process.argv[2], process.argv[3]).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
