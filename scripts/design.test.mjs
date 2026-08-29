import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

// Exercise the actual SVG helpers without adding a browser test dependency.
const source = await readFile(new URL("../src/lib/silhouette.ts", import.meta.url), "utf8");
const { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2021 },
});
const { barPath, bubbleTailPath } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`
);
const points = (path) => path.match(/-?\d+(?:\.\d+)?(?:e[+-]?\d+)?/gi).map(Number);

test("the rail starts and finishes at the bezel, without a horizontal shelf", () => {
  const d = barPath(71, 358, 31, 9, false);
  const p = points(d);
  assert.deepEqual(p.slice(0, 2), [71, 0]);
  assert.deepEqual(p.slice(6, 8), [40, 31]);
  assert.deepEqual(p.slice(12, 14), [9, 62]);
  assert.deepEqual(p.slice(14, 16), [9, 296]);
  assert.deepEqual(p.slice(-4), [71, 358, 71, 0]);
  // Each join has a shared tangent, preventing a kink in the black outline.
  assert.equal(p[1], 0);
  assert.equal(p[0], p[2]);
  assert.equal(p[5], p[7]);
  assert.equal(p[7], p[9]);
  assert.equal(p[10], p[12]);
});

test("left edge is an exact horizontal reflection, including the end caps", () => {
  const right = points(barPath(71, 358, 31, 9, false));
  const left = points(barPath(71, 358, 31, 9, true));
  assert.equal(left.length, right.length);
  right.forEach((value, i) => {
    assert.ok(Math.abs(left[i] - (i % 2 ? value : 71 - value)) < 1e-9);
  });
});

test("large radii cannot make the end caps overlap in a short rail", () => {
  const p = points(barPath(71, 40, 80, 9, false));
  p.forEach((value, i) => {
    assert.ok(Number.isFinite(value));
    assert.ok(value >= 0 && value <= (i % 2 ? 40 : 71));
  });
  assert.deepEqual(p.slice(12, 16), [9, 20, 9, 20]);
  assert.equal(barPath(71, 100, 0, 9, false), "M 9,0 H 71 V 100 H 9 Z");
});

test("bubble tail is centred and has vertical tangents where it joins the body", () => {
  const p = points(bubbleTailPath(26, 46));
  assert.deepEqual(p.slice(0, 2), [0, 0]);
  assert.deepEqual(p.slice(6, 8), [26, 23]);
  assert.deepEqual(p.slice(-2), [0, 46]);
  assert.equal(p[2], 0);
  assert.equal(p[10], 0);
  assert.equal(p[4], p[8]);
  assert.ok(Math.abs(p[5] + p[9] - 46) < 1e-9);
});
