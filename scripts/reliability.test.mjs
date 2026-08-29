import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
import { isOwned } from "./dev.mjs";

async function load(name) {
  const text = await readFile(new URL(`../src/lib/${name}.ts`, import.meta.url), "utf8");
  const { outputText } = ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2021 } });
  return import(`data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`);
}
const { isStale, menuPosition, readingStatus } = await load("freshness");
const { resetText } = await load("format");
const { subscribe } = await load("subscriptions");
const snapshot = { stale: false, observed_at: 1000, checked_at: 1000, stale_after_seconds: 120, primary: { percent: 20, resets_at: 1600 }, source: "cli" };

test("age expires locally even if the backend never sends another event", () => {
  assert.equal(isStale(snapshot, 1119), false);
  assert.equal(isStale(snapshot, 1120), true);
  assert.equal(isStale({ ...snapshot, observed_at: null }, 1000), true);
  assert.equal(isStale({ ...snapshot, observed_at: 1006 }, 1000), true);
});
test("reset expiry requires a new reading; the old percentage is not reset to zero", () => {
  const after = { ...snapshot, observed_at: 1599 };
  assert.equal(isStale(after, 1600), true);
  assert.equal(resetText(1600, 1601), "Awaiting reset update");
  assert.equal(after.primary.percent, 20);
});
test("reading provenance and countdown advance on the UI clock", () => {
  assert.equal(readingStatus(snapshot, 1009), "CLI reading · 9s ago");
  assert.equal(readingStatus({ ...snapshot, source: "file" }, 1120), "Local snapshot · 2m ago");
  assert.equal(resetText(1600, 1300), "Resets in 5 min");
  assert.equal(resetText(1600, 1360), "Resets in 4 min");
});
test("context menu stays inside all four window corners", () => {
  for (const x of [0, 305]) for (const y of [0, 617]) {
    const p = menuPosition(x, y, 176, 120, 306, 618);
    assert.ok(p.x >= 6 && p.x + 176 <= 300);
    assert.ok(p.y >= 6 && p.y + 120 <= 612);
  }
});
test("listeners resolving after unmount are immediately removed", async () => {
  let finish, removed = 0, bootstrapped = false;
  const pending = new Promise((resolve) => { finish = resolve; });
  const cleanup = subscribe([() => pending], async () => { bootstrapped = true; }, assert.fail);
  cleanup();
  finish(() => removed++);
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(removed, 1);
  assert.equal(bootstrapped, false);
});
test("bootstrap waits for listeners and errors are handled", async () => {
  const order = [];
  const cleanup = subscribe([async () => { order.push("listen"); return () => order.push("unlisten"); }], async () => { order.push("bootstrap"); throw new Error("offline"); }, (e) => order.push(e.message));
  await new Promise((resolve) => setImmediate(resolve));
  cleanup();
  assert.deepEqual(order, ["listen", "bootstrap", "offline", "unlisten"]);
});
test("dev stop rejects unrelated ports, generic Tauri processes, and reused PIDs", () => {
  const script = new URL("./dev.mjs", import.meta.url).pathname;
  const record = { pid: 12345, script, token: "8b74f4cd-1ff3-4a5f-ab14-1684528526ae" };
  assert.equal(isOwned(record, `node ${script} worker ${record.token}`), true);
  for (const command of ["node vite --port 5173", "npm run tauri dev", `node ${script} worker wrong-token`]) assert.equal(isOwned(record, command), false);
  assert.equal(isOwned({ ...record, pid: -1 }, `node ${script} worker ${record.token}`), false);
});
