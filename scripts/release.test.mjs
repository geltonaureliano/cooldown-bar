import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { assetNames, sha256, verifyArtifacts, writeManifest } from "./ci/artifacts.mjs";
import { ApiError, GitHub } from "./ci/github.mjs";
import { appleSecrets, buildEnvironment } from "./ci/macos.mjs";
import { classifyCommit, parseGitLog, prepareRelease, previousTag, publishRelease, releaseRequested, renderNotes } from "./ci/release.mjs";
import { compareVersions, nextVersion, parseVersion, readVersions, setVersion, versionFiles } from "./ci/version.mjs";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const commit = "a".repeat(40), other = "b".repeat(40), version = "0.0.1", repository = "example/cooldown-bar";
const managedBody = `<!-- cooldown-bar-release:${commit} -->`;
const metadata = { version, commit, tag: `v${version}`, architectures: ["arm64", "x86_64"], minimumMacOS: "13.0", signing: "adhoc" };
function temp(t) {
  const path = mkdtempSync(join(tmpdir(), "cooldown-bar-release-test-"));
  t.after(() => rmSync(path, { recursive: true, force: true }));
  return path;
}
function fixture(t, overrides = {}) {
  const path = temp(t);
  for (const name of assetNames(version).slice(0, 2)) writeFileSync(join(path, name), `test content for ${name}`);
  writeManifest(path, { ...metadata, ...overrides });
  return path;
}
class FakeGitHub {
  constructor({ target = null, draft = null, failUpload = false, corrupt = false, failNotes = false } = {}) {
    this.target = target;
    this.draft = draft;
    this.uploaded = [];
    this.calls = [];
    this.failUpload = failUpload;
    this.corrupt = corrupt;
    this.failNotes = failNotes;
  }
  async release() { return this.draft; }
  async tagCommit() { return this.target; }
  async assets() { return [...this.uploaded]; }
  async request(method, path, body) {
    this.calls.push({ method, path, body });
    if (path === "/releases/latest") return this.latest ?? null;
    if (path === "/releases/generate-notes") {
      if (this.failNotes) throw new ApiError(503, method, path);
      return { body: "### Features\n* A merged PR by @contributor" };
    }
    if (path === "/git/refs") { this.target = body.sha; return {}; }
    if (path === "/releases") { this.draft = { id: 1, html_url: "https://github.com/example/cooldown-bar/releases/tag/v0.0.1", ...body }; return this.draft; }
    if (method === "DELETE") { this.uploaded = this.uploaded.filter((a) => a.id !== Number(path.split("/").at(-1))); return null; }
    if (method === "POST" && path.includes("/assets?")) {
      if (this.failUpload) throw new Error("Simulated failed upload");
      const name = new URL(`https://uploads.github.com${path}`).searchParams.get("name");
      const asset = { name, size: body.length, digest: `sha256:${this.corrupt ? "0".repeat(64) : sha256(body)}`, state: "uploaded", id: this.calls.length };
      this.uploaded.push(asset);
      return asset;
    }
    if (method === "PATCH" && path === "/releases/1") { Object.assign(this.draft, body); return this.draft; }
    throw new Error(`Unexpected mock call: ${method} ${path}`);
  }
}
const publishing = (directory) => ({ version, commit, previous: "", repository, directory, commits: [{ sha: commit, subject: "feat: first version", body: "" }], curated: "First release." });

test("stable versions reject injection, prefixes, leading zeroes and invalid numbers", () => {
  for (const value of ["v0.0.1", "01.0.1", "0.0.1-rc.1", "0.0", "0.0.1\npublish=true", "1.2.3;echo x", "999999999999999999.0.0", undefined]) {
    assert.throws(() => parseVersion(value));
  }
  assert.deepEqual(parseVersion("0.0.1"), [0, 0, 1]);
  assert.ok(compareVersions("0.0.10", "0.0.9") > 0);
});
test("explicit/semantic bumps preserve monotonic versions", () => {
  assert.equal(nextVersion("0.0.1", "patch"), "0.0.2");
  assert.equal(nextVersion("0.8.9", "minor"), "0.9.0");
  assert.equal(nextVersion("0.8.9", "major"), "1.0.0");
  assert.equal(nextVersion("0.0.1", "0.2.0"), "0.2.0");
  assert.throws(() => nextVersion("0.1.0", "0.0.1"));
  assert.throws(() => nextVersion("0.1.0", "0.1.0"));
});
test("all five version files stay aligned without changing dependency versions", (t) => {
  const directory = temp(t);
  for (const name of versionFiles) cpSync(join(root, name), join(directory, name), { recursive: true });
  const current = readVersions(directory);
  const oldLock = JSON.parse(readFileSync(join(directory, "package-lock.json"), "utf8"));
  const next = setVersion("patch", directory);
  assert.equal(next, nextVersion(current, "patch"));
  assert.equal(readVersions(directory), next);
  const newLock = JSON.parse(readFileSync(join(directory, "package-lock.json"), "utf8"));
  delete oldLock.packages[""]; delete newLock.packages[""];
  assert.deepEqual(newLock.packages, oldLock.packages);
  const cargoBefore = readFileSync(join(root, "src-tauri/Cargo.lock"), "utf8");
  const cargoAfter = readFileSync(join(directory, "src-tauri/Cargo.lock"), "utf8");
  assert.equal(cargoAfter, cargoBefore.replace(`name = "cooldown-bar"\nversion = "${current}"`, `name = "cooldown-bar"\nversion = "${next}"`));
});
test("version mismatch fails before any version file is rewritten", (t) => {
  const directory = temp(t);
  for (const name of versionFiles) cpSync(join(root, name), join(directory, name), { recursive: true });
  const file = join(directory, "package.json"), original = readFileSync(file, "utf8");
  const data = JSON.parse(original);
  data.version = "99.0.0";
  writeFileSync(file, JSON.stringify(data));
  assert.throws(() => readVersions(directory), /differ/);
  assert.throws(() => setVersion("patch", directory), /differ/);
  assert.equal(readFileSync(join(directory, "src-tauri/Cargo.lock"), "utf8"), readFileSync(join(root, "src-tauri/Cargo.lock"), "utf8"));
});

test("publication is limited to the default branch and explicit manual intent", () => {
  const input = { defaultBranch: "trunk", ref: "refs/heads/trunk", publish: false };
  assert.equal(releaseRequested({ ...input, eventName: "push" }), true);
  assert.equal(releaseRequested({ ...input, eventName: "pull_request" }), false);
  assert.equal(releaseRequested({ ...input, eventName: "pull_request_target", publish: true }), false);
  assert.equal(releaseRequested({ ...input, eventName: "workflow_dispatch" }), false);
  assert.equal(releaseRequested({ ...input, eventName: "workflow_dispatch", publish: true }), true);
  assert.equal(releaseRequested({ ...input, eventName: "push", ref: "refs/heads/feature" }), false);
  assert.throws(() => releaseRequested({ ...input, eventName: "workflow_dispatch", publish: true, ref: "refs/heads/feature" }), /default branch/);
});
test("previous tag selection uses numeric versions and ignores non-release tags", () => {
  assert.equal(previousTag(["v0.0.9", "nightly", "v0.0.10", "v0.0.11", "v0.0.11-rc.1"], "0.0.11"), "v0.0.10");
  assert.equal(previousTag([], "0.0.1"), "");
});
test("PR/dry-run preparation needs no GitHub token or API calls", async () => {
  const result = await prepareRelease({ version, commit, requested: false, tags: [] }, null);
  assert.equal(result.publish, false);
});
test("first release is v0.0.1 and preparation has no remote writes", async () => {
  const api = new FakeGitHub();
  const result = await prepareRelease({ version, commit, requested: true, tags: [] }, api);
  assert.equal(result.tag, "v0.0.1");
  assert.equal(result.publish, true);
  assert.equal(api.calls.length, 0);
});
test("pushes at an already published version validate without republishing", async () => {
  const api = new FakeGitHub({ target: other, draft: { draft: false } });
  assert.equal((await prepareRelease({ version, commit, requested: true, tags: ["v0.0.1"] }, api)).publish, false);
});
test("tag conflicts and old versions fail before building/publishing", async () => {
  const input = { version, commit, requested: true, tags: [] };
  await assert.rejects(prepareRelease(input, new FakeGitHub({ target: other })), /another commit/);
  await assert.rejects(prepareRelease({ ...input, tags: ["v0.1.0"] }, new FakeGitHub()), /older/);
});
test("foreign drafts are not overwritten by the pipeline", async (t) => {
  const api = new FakeGitHub({ target: commit, draft: { id: 1, draft: true, body: "Handwritten release" } });
  await assert.rejects(prepareRelease({ version, commit, requested: true, tags: [] }, api), /draft/);
  await assert.rejects(publishRelease(publishing(fixture(t)), api), /draft/);
  assert.equal(api.calls.length, 0);
});
test("commit parser handles real Git history, multiline bodies and shell-like text", (t) => {
  const directory = temp(t);
  const git = (...args) => execFileSync("git", ["-C", directory, ...args], { encoding: "utf8" }).trim();
  git("init", "--quiet");
  git("-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false", "commit", "--quiet", "--allow-empty", "-m", "feat(ui): show $(text)", "-m", "BREAKING CHANGE: revised config");
  const result = parseGitLog(git("log", "--format=%H%x00%B%x00"));
  assert.equal(result.length, 1);
  assert.equal(result[0].subject, "feat(ui): show $(text)");
  assert.match(result[0].body, /BREAKING CHANGE/);
});
test("release notes classify features, fixes, security, dependencies and breaking changes", () => {
  const cases = [
    ["feat(ui): new ring", "Features"], ["fix: timeout", "Fixes"], ["perf: less CPU", "Performance"],
    ["fix(security): validate input", "Security"], ["chore(deps): update Tauri", "Dependencies"],
    ["ci: release workflow", "Build and automation"], ["feat!: new schema", "Breaking changes"],
    ["Unstructured commit", "Other changes"],
  ];
  for (const [subject, category] of cases) assert.equal(classifyCommit({ subject }).category, category);
  assert.equal(classifyCommit({ subject: "fix: config", body: "BREAKING-CHANGE: needs migration" }).breaking, true);
});
test("notes work without PRs or previous tags, escape commit titles and disclose ad-hoc signing", () => {
  const notes = renderNotes({ version, commit, repository, commits: [{ sha: commit, subject: "fix: <script> [link](bad)" }], curated: "First native release." });
  assert.match(notes, /First public release/);
  assert.match(notes, /First native release/);
  assert.match(notes, /\\<script\\>/);
  assert.match(notes, /no Apple notarization/);
  assert.match(notes, /Cooldown_Bar_0.0.1_universal.dmg/);
  assert.ok(!notes.includes("undefined"));
  assert.ok(!notes.includes("Apple ticket verified"));
});
test("signed notes and previous-version comparison reflect verified build metadata", () => {
  const notes = renderNotes({ version: "0.0.2", commit, repository, previous: "v0.0.1", commits: [], signing: "developer-id-notarized", generated: "### PRs\n- Improved widget" });
  assert.match(notes, /compare\/v0.0.1\.\.\.v0.0.2/);
  assert.match(notes, /Apple ticket verified/);
  assert.match(notes, /Improved widget/);
});
test("artifact checks validate exact names, commit, version, architectures and checksums", (t) => {
  const directory = fixture(t);
  assert.equal(verifyArtifacts(directory, { version, commit }).assets.length, 4);
  assert.throws(() => verifyArtifacts(directory, { version, commit: other }), /does not match/);
  writeFileSync(join(directory, assetNames(version)[0]), "corrupt download");
  assert.throws(() => verifyArtifacts(directory, { version, commit }), /integrity/);
});
test("artifacts with extra files, symlinks, wrong platform or forged checksums are refused", (t) => {
  const extra = fixture(t);
  writeFileSync(join(extra, ".secret"), "do not upload");
  assert.throws(() => verifyArtifacts(extra, { version, commit }), /unexpected/);
  const linked = fixture(t), name = assetNames(version)[0];
  rmSync(join(linked, name));
  symlinkSync(join(linked, "build-info.json"), join(linked, name));
  assert.throws(() => verifyArtifacts(linked, { version, commit }), /Invalid file/);
  const wrong = fixture(t, { architectures: ["arm64"] });
  assert.throws(() => verifyArtifacts(wrong, { version, commit }), /does not match/);
  const checksums = fixture(t);
  writeFileSync(join(checksums, "SHA256SUMS.txt"), "bad hashes\n");
  assert.throws(() => verifyArtifacts(checksums, { version, commit }), /SHA256SUMS/);
});
test("publication creates tag at the tested commit, uploads four assets, then publishes last", async (t) => {
  const api = new FakeGitHub();
  const result = await publishRelease(publishing(fixture(t)), api);
  assert.equal(api.target, commit);
  assert.equal(api.uploaded.length, 4);
  assert.equal(api.calls.at(-1).body.draft, false);
  assert.equal(api.draft.draft, false);
  assert.equal(result.alreadyPublished, false);
});
test("invalid local artifacts never create a remote tag or draft", async (t) => {
  const api = new FakeGitHub(), directory = fixture(t, { commit: other });
  await assert.rejects(publishRelease(publishing(directory), api), /does not match/);
  assert.equal(api.calls.length, 0);
  assert.equal(api.target, null);
});
test("failed uploads leave a draft, and reruns resume without moving the tag", async (t) => {
  const api = new FakeGitHub({ failUpload: true }), input = publishing(fixture(t));
  await assert.rejects(publishRelease(input, api), /failed upload/);
  assert.equal(api.draft.draft, true);
  assert.equal(api.target, commit);
  api.failUpload = false;
  const result = await publishRelease(input, api);
  assert.equal(result.alreadyPublished, false);
  assert.equal(api.draft.draft, false);
  assert.equal(api.calls.filter((c) => c.path === "/git/refs").length, 1);
});
test("draft reruns reuse matching assets and repair incomplete uploads only in drafts", async (t) => {
  const directory = fixture(t), assets = verifyArtifacts(directory, { version, commit }).assets;
  const api = new FakeGitHub({ target: commit, draft: { id: 1, draft: true, body: managedBody, html_url: "https://github.com/example/cooldown-bar/releases/tag/v0.0.1" } });
  api.uploaded = [{ id: 10, name: assets[0].name, digest: assets[0].digest, size: assets[0].size, state: "uploaded" }, { id: 11, name: assets[1].name, digest: null, size: 0, state: "starter" }];
  await publishRelease(publishing(directory), api);
  assert.equal(api.calls.filter((c) => c.path.includes("/assets?")).length, 3);
  assert.deepEqual(api.calls.filter((c) => c.method === "DELETE").map((c) => c.path), ["/releases/assets/11"]);
});
test("published releases are immutable to this workflow, including on retries", async (t) => {
  const api = new FakeGitHub({ target: commit, draft: { draft: false, html_url: "https://github.com/example/cooldown-bar/releases/tag/v0.0.1" } });
  assert.equal((await publishRelease(publishing(fixture(t)), api)).alreadyPublished, true);
  assert.equal(api.calls.length, 0);
  api.target = other;
  await assert.rejects(publishRelease(publishing(fixture(t)), api), /another commit/);
  assert.equal(api.calls.length, 0);
});
test("GitHub upload digest mismatch blocks public release", async (t) => {
  const api = new FakeGitHub({ corrupt: true });
  await assert.rejects(publishRelease(publishing(fixture(t)), api), /Upload verification/);
  assert.equal(api.draft.draft, true);
  assert.ok(!api.calls.some((c) => c.body?.draft === false));
});
test("retrying an old failed run cannot demote a newer latest release", async (t) => {
  const api = new FakeGitHub();
  api.latest = { tag_name: "v0.0.2" };
  await assert.rejects(publishRelease(publishing(fixture(t)), api), /newer version/);
  assert.equal(api.target, null);
  assert.ok(api.calls.every((c) => c.method === "GET"));
});
test("a tag changed during uploads leaves the release in draft", async (t) => {
  const api = new FakeGitHub();
  let reads = 0;
  api.tagCommit = async () => ++reads === 1 ? null : other;
  await assert.rejects(publishRelease(publishing(fixture(t)), api), /tag changed/);
  assert.equal(api.draft.draft, true);
});
test("GitHub notes failure falls back to local history without inventing changes", async (t) => {
  const api = new FakeGitHub({ failNotes: true });
  await publishRelease(publishing(fixture(t)), api);
  assert.match(api.draft.body, /first version/);
  assert.ok(!api.draft.body.includes("merged PR"));
});
test("API reads retry transient failures; a failed write is never blindly retried", async () => {
  let attempts = 0;
  const api = new GitHub(repository, "masked-test-token", { sleep: async () => {}, fetchImpl: async () => {
    attempts++;
    return new Response(attempts < 3 ? "temporary" : '{"ok":true}', { status: attempts < 3 ? 503 : 200 });
  } });
  assert.equal((await api.request("GET", "/releases")).ok, true);
  assert.equal(attempts, 3);
  attempts = 0;
  await assert.rejects(api.request("POST", "/releases", {}), (e) => e instanceof ApiError && !e.message.includes("masked-test-token"));
  assert.equal(attempts, 1);
});
test("API distinguishes not-found, permission errors and uncertain network writes", async () => {
  const api = new GitHub(repository, "test-token", { fetchImpl: async (url) => url.includes("/releases?") ? new Response("[]", { status: 200 }) : new Response("", { status: 404 }) });
  assert.equal(await api.release("v0.0.1"), null);
  api.fetch = async () => new Response("", { status: 403 });
  await assert.rejects(api.release("v0.0.1"), (e) => e.status === 403);
  let calls = 0;
  api.fetch = async () => { calls++; throw new Error("network"); };
  await assert.rejects(api.request("POST", "/git/refs", {}), /Run the workflow again/);
  assert.equal(calls, 1);
});
test("annotated tags resolve to their underlying commit", async () => {
  const api = new GitHub(repository, "test-token", { fetchImpl: async (url) => new Response(JSON.stringify(url.includes("/git/ref/") ? { object: { type: "tag", sha: other } } : { object: { type: "commit", sha: commit } }), { status: 200 }) });
  assert.equal(await api.tagCommit("v0.0.1"), commit);
});
test("draft lookup paginates the release list when the tag endpoint returns 404", async () => {
  const draft = { id: 42, tag_name: "v0.0.1", draft: true };
  const api = new GitHub(repository, "test-token", { fetchImpl: async (url) => {
    if (url.includes("/releases/tags/")) return new Response("", { status: 404 });
    return new Response(JSON.stringify(url.endsWith("page=1") ? Array.from({ length: 100 }, () => ({ tag_name: "other" })) : [draft]), { status: 200 });
  } });
  assert.deepEqual(await api.release("v0.0.1"), draft);
});
test("ad-hoc builds strip Apple credentials; incomplete signed configuration fails closed", () => {
  const secrets = Object.fromEntries(appleSecrets.map((key) => [key, "test-secret"]));
  secrets.APPLE_SIGNING_IDENTITY = "Developer ID Application: Example (TEAMID)";
  const result = buildEnvironment({ ...secrets, SIGNING_ENABLED: "false", APPLE_API_KEY: "other-secret", PATH: "/bin" });
  assert.equal(result.signing, "adhoc");
  assert.equal(result.env.APPLE_SIGNING_IDENTITY, "-");
  for (const key of appleSecrets.filter((key) => key !== "APPLE_SIGNING_IDENTITY")) assert.equal(result.env[key], undefined);
  assert.equal(result.env.APPLE_API_KEY, undefined);
  assert.equal(result.env.PATH, "/bin");
  assert.equal(buildEnvironment({ ...secrets, SIGNING_ENABLED: "true" }).signing, "developer-id-notarized");
  assert.throws(() => buildEnvironment({ SIGNING_ENABLED: "true" }), /secrets are missing/);
  assert.throws(() => buildEnvironment({ ...secrets, SIGNING_ENABLED: "true", APPLE_SIGNING_IDENTITY: "-" }), /Developer ID/);
});
test("workflow uses pinned actions, locked dependencies and no privileged PR triggers", () => {
  const workflow = readFileSync(join(root, ".github/workflows/ci-release.yml"), "utf8");
  const actions = [...workflow.matchAll(/uses:\s*([^\s]+)\s/g)].map((m) => m[1]);
  assert.ok(actions.length >= 8);
  for (const action of actions) assert.match(action, /@[a-f0-9]{40}$/);
  assert.ok(!workflow.includes("pull_request_target:"));
  assert.ok(!workflow.includes("workflow_run:"));
  assert.match(workflow, /npm ci/);
  assert.match(workflow, /cargo test --locked/);
  assert.match(workflow, /digest-mismatch: error/);
  assert.equal((workflow.match(/contents: write/g) ?? []).length, 1);
});
