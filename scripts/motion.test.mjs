import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";
const source = await readFile(new URL("../src/lib/motion.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2021 } });
const { circleCurves, createMotionChannel, liquidLevel, morphPath, orbDiameter, railCurves, spring } = await import("data:text/javascript;base64," + Buffer.from(outputText).toString("base64"));
const numbers = (path) => path.match(/-?\d+(?:\.\d+)?/g).map(Number);

test("both docked outlines keep the exact bounds and morph into the same orb", () => {
  const right = railCurves(62, 268, 31, "right"), left = railCurves(62, 268, 31, "left");
  for (const [edge, curves] of [["right", right], ["left", left]]) {
    const values = numbers(morphPath(right, left, circleCurves(31, 100, 34), 0, edge === "left" ? 1 : 0));
    assert.equal(Math.min(...values.filter((_, i) => i % 2 === 0)), 0);
    assert.equal(Math.max(...values.filter((_, i) => i % 2 === 0)), 62);
    assert.equal(Math.min(...values.filter((_, i) => i % 2 === 1)), 0);
    assert.equal(Math.max(...values.filter((_, i) => i % 2 === 1)), 268);
  }
  assert.equal(morphPath(right, left, circleCurves(31, 100, 34), 1, 0), morphPath(right, left, circleCurves(31, 100, 34), 1, 1));
});

test("orb size stays compact across supported custom bar widths", () => {
  assert.equal(orbDiameter(24), 54); assert.equal(orbDiameter(62), 68); assert.equal(orbDiameter(400), 76);
});

test("liquid remains visible and bounded for missing, low, and high readings", () => {
  assert.equal(liquidLevel(null), 46); assert.equal(liquidLevel(-20), 24);
  assert.equal(liquidLevel(0), 24); assert.equal(liquidLevel(100), 86); assert.equal(liquidLevel(200), 86);
});

test("critical spring converges without overshooting and is frame-rate independent", () => {
  const run = (dt) => { let x = 0, v = 0; for (let t = 0; t < 1; t += dt) [x, v] = spring(x, v, 1, dt); return x; };
  const sixty = run(1 / 60), high = run(1 / 120);
  assert.ok(sixty > .999 && sixty <= 1); assert.ok(Math.abs(sixty - high) < 0.0001);
});

test("motion channel rejects stale frames and unsubscribes cleanly", () => {
  const channel = createMotionChannel(); let seen = 0;
  const unsubscribe = channel.subscribe(() => seen++);
  assert.equal(channel.publish({ revision: 2 }), true);
  assert.equal(channel.publish({ revision: 1 }), false);
  unsubscribe(); channel.publish({ revision: 3 }); assert.equal(seen, 1);
});
