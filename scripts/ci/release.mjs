import { execFileSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { GitHub } from "./github.mjs";
import { assetNames, verifyArtifacts } from "./artifacts.mjs";
import { compareVersions, parseVersion, readVersions } from "./version.mjs";

const git = (...args) => execFileSync("git", args, { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }).trim();
const escapeMarkdown = (text) => text.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/([\\`*_[\]<>])/g, "\\$1");
const marker = (commit) => `<!-- cooldown-bar-release:${commit} -->`;

export function releaseRequested({ eventName, ref, defaultBranch, publish }) {
  const main = ref === `refs/heads/${defaultBranch}`;
  if (eventName === "workflow_dispatch" && publish && !main) throw new Error("Manual publication is allowed only from the default branch.");
  return main && (eventName === "push" || (eventName === "workflow_dispatch" && publish));
}

export function previousTag(tags, version) {
  return tags.filter((tag) => /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag))
    .filter((tag) => compareVersions(tag.slice(1), version) < 0)
    .sort((a, b) => compareVersions(b.slice(1), a.slice(1)))[0] ?? "";
}

export async function prepareRelease({ version, commit, requested, tags }, api) {
  parseVersion(version);
  if (!/^[a-f0-9]{40}$/.test(commit)) throw new Error("Invalid commit.");
  const tag = `v${version}`, previous = previousTag(tags, version);
  const metadata = { version, commit, tag, previous_tag: previous, publish: false };
  if (!requested) return { ...metadata, reason: "Validation without publication." };
  const release = await api.release(tag);
  // Normal pushes after a release keep validating the code without republishing it.
  if (release && !release.draft) return { ...metadata, reason: `${tag} is already published. Change the version before creating another release.` };
  for (const other of tags) {
    if (/^v\d+\.\d+\.\d+$/.test(other) && compareVersions(other.slice(1), version) > 0) {
      throw new Error(`Version ${version} is older than tag ${other} and will not be published as latest.`);
    }
  }
  const target = await api.tagCommit(tag);
  if (target && target !== commit) throw new Error(`A tag ${tag} already points to another commit. Increase the version because tags are never moved.`);
  if (release && !release.body?.includes(marker(commit))) throw new Error("An unmanaged draft already exists. Review it manually before continuing.");
  return { ...metadata, publish: true, reason: `Publish ${tag} after all tests and the build.` };
}

export function parseGitLog(text) {
  const fields = text.split("\0"), commits = [];
  for (let i = 0; i + 1 < fields.length; i += 2) {
    const sha = fields[i].trim(), message = fields[i + 1].trim();
    if (!/^[a-f0-9]{40}$/.test(sha)) throw new Error("Invalid Git history.");
    const [subject, ...body] = message.split("\n");
    commits.push({ sha, subject, body: body.join("\n") });
  }
  return commits;
}

export function classifyCommit({ subject, body = "" }) {
  const match = subject.match(/^([a-z]+)(?:\(([^)]+)\))?(!)?:\s*(.+)$/i);
  const type = match?.[1]?.toLowerCase(), scope = match?.[2]?.toLowerCase();
  const breaking = Boolean(match?.[3]) || /^BREAKING[ -]CHANGE:/m.test(body);
  let category = "Other changes";
  if (breaking) category = "Breaking changes";
  else if (type === "security" || scope === "security") category = "Security";
  else if (scope === "deps" || scope === "deps-dev" || type === "deps") category = "Dependencies";
  else category = ({ feat: "Features", fix: "Fixes", perf: "Performance", docs: "Documentation", ci: "Build and automation", build: "Build and automation", refactor: "Maintenance", test: "Tests", chore: "Maintenance" })[type] ?? category;
  return { category, breaking, title: match ? `${match[2] ? `${match[2]}: ` : ""}${match[4]}` : subject };
}

const categoryOrder = ["Breaking changes", "Security", "Features", "Fixes", "Performance", "Dependencies", "Build and automation", "Documentation", "Tests", "Maintenance", "Other changes"];

export function renderNotes({ version, commit, previous, repository, commits, curated = "", generated = "", signing = "adhoc" }) {
  parseVersion(version);
  const repo = `https://github.com/${repository}`, tag = `v${version}`;
  const grouped = new Map();
  for (const entry of commits.slice(0, 200)) {
    const item = classifyCommit(entry);
    if (!grouped.has(item.category)) grouped.set(item.category, []);
    grouped.get(item.category).push(`1. ${escapeMarkdown(item.title)} ([${entry.sha.slice(0, 7)}](${repo}/commit/${entry.sha}))`);
  }
  const historyLink = previous ? `${repo}/compare/${previous}...${tag}` : `${repo}/commits/${tag}`;
  const parts = [marker(commit), `# Cooldown Bar ${tag}`, previous ? `Changes since **${previous}**.` : "First public release of Cooldown Bar."];
  if (curated.trim()) parts.push("## Highlights", curated.trim());
  if (generated.trim()) parts.push("## Pull requests and contributors", generated.trim());
  parts.push("<details>\n<summary>Changes by commit</summary>\n");
  for (const category of categoryOrder) if (grouped.has(category)) parts.push(`### ${category}`, grouped.get(category).join("\n"));
  if (!commits.length) parts.push("No additional commits in this release range.");
  if (commits.length > 200) parts.push("The first 200 commits are shown. Use the complete history link below.");
  parts.push("</details>", `[Complete history](${historyLink})`);
  const names = assetNames(version);
  parts.push("## Download and installation", "Compatible with **macOS 13 or newer** on **Apple Silicon and Intel**. The same package supports both architectures.",
    `1. [DMG installer](${repo}/releases/download/${tag}/${names[0]}): open it and move Cooldown Bar to Applications.`,
    `1. [Application ZIP](${repo}/releases/download/${tag}/${names[1]}): an alternative to the DMG.`,
    `1. [SHA256SUMS.txt](${repo}/releases/download/${tag}/SHA256SUMS.txt) and [build metadata](${repo}/releases/download/${tag}/build-info.json).`,
    signing === "developer-id-notarized"
      ? "**Signing:** Developer ID with notarization and Apple ticket verified by CI."
      : "**Test build with ad hoc signing and no Apple notarization.** Gatekeeper can block it. Use macOS Privacy and Security options only when you trust the source. This signature does not verify an Apple developer identity.",
    "To verify integrity, download both packages, `build-info.json` and `SHA256SUMS.txt` to the same directory and run:\n\n```bash\nshasum -a 256 -c SHA256SUMS.txt\n```",
    `Compiled commit: [\`${commit}\`](${repo}/commit/${commit}).`,
    "Categories are inferred from commit messages and pull request labels. They are not an automatic analysis of code behavior.");
  const notes = `${parts.join("\n\n")}\n`;
  if (Buffer.byteLength(notes) > 120_000) throw new Error("Release notes are too long. Shorten the editorial summary.");
  return notes;
}

export async function publishRelease({ version, commit, previous, repository, directory, commits, curated }, api) {
  // No remote mutations occur before all artifacts and provenance are verified.
  const { assets, info } = verifyArtifacts(directory, { version, commit });
  const tag = `v${version}`;
  let release = await api.release(tag);
  const target = await api.tagCommit(tag);
  if (target && target !== commit) throw new Error(`A tag ${tag} points to another commit. Publication refused.`);
  if (release && !release.draft) {
    if (target !== commit) throw new Error("The existing release does not match the compiled commit.");
    return { url: release.html_url, alreadyPublished: true };
  }
  if (release && !release.body?.includes(marker(commit))) throw new Error("The existing draft does not belong to this run and commit.");
  // A failed old run can be retried after a newer version has already shipped.
  // Recheck remotely here, even when the preparation job was not rerun.
  const latest = await api.request("GET", "/releases/latest", undefined, { allow404: true });
  if (latest?.tag_name && /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(latest.tag_name) && compareVersions(latest.tag_name.slice(1), version) > 0) {
    throw new Error(`A newer version (${latest.tag_name}) has already been published. This older run will not become latest.`);
  }
  let generated = "";
  try {
    const response = await api.request("POST", "/releases/generate-notes", {
      tag_name: tag, target_commitish: commit, configuration_file_path: ".github/release.yml",
      ...(previous ? { previous_tag_name: previous } : {}),
    });
    generated = response.body;
  } catch {
    console.warn("Pull request notes are unavailable. Local commits and editorial notes will be used.");
  }
  const body = renderNotes({ version, commit, previous, repository, commits, curated, generated, signing: info.signing });
  if (!target) await api.request("POST", "/git/refs", { ref: `refs/tags/${tag}`, sha: commit });
  if (!release) release = await api.request("POST", "/releases", { tag_name: tag, target_commitish: commit, name: `Cooldown Bar ${tag}`, body, draft: true, prerelease: false });
  else release = await api.request("PATCH", `/releases/${release.id}`, { body, name: `Cooldown Bar ${tag}` });

  const existing = await api.assets(release.id);
  if (existing.some((a) => !assets.some((expected) => a.name === expected.name))) throw new Error("The draft contains additional files. Manual review is required.");
  for (const asset of assets) {
    const old = existing.find((a) => a.name === asset.name);
    if (old?.state === "uploaded" && old.digest === asset.digest && old.size === asset.size) continue;
    // Only this CI's draft can be repaired. Published assets are never replaced.
    if (old) await api.request("DELETE", `/releases/assets/${old.id}`);
    await api.request("POST", `/releases/${release.id}/assets?name=${encodeURIComponent(asset.name)}`, asset.data, { binary: true });
  }
  const uploaded = await api.assets(release.id);
  if (uploaded.length !== assets.length || assets.some((expected) => !uploaded.some((actual) => actual.name === expected.name && actual.size === expected.size && actual.digest === expected.digest && actual.state === "uploaded"))) {
    throw new Error("Upload verification failed. The release remains a draft.");
  }
  if (await api.tagCommit(tag) !== commit) throw new Error("The tag changed during publication. The draft was preserved.");
  // Immutable releases remain compatible: all uploads happen before publishing.
  const published = await api.request("PATCH", `/releases/${release.id}`, { draft: false, prerelease: false, make_latest: "true" });
  return { url: published.html_url, alreadyPublished: false };
}

async function main() {
  const command = process.argv[2], version = readVersions(), commit = git("rev-parse", "HEAD");
  const tags = git("tag", "--merged", commit).split("\n").filter(Boolean);
  const previous = previousTag(tags, version);
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, "utf8"));
  const requested = releaseRequested({ eventName: process.env.GITHUB_EVENT_NAME, ref: process.env.GITHUB_REF, defaultBranch: event.repository.default_branch, publish: event.inputs?.publish === "true" || event.inputs?.publish === true });
  const api = requested ? new GitHub(process.env.GITHUB_REPOSITORY, process.env.GITHUB_TOKEN) : null;
  if (command === "prepare") {
    const metadata = await prepareRelease({ version, commit, requested, tags }, api);
    for (const [key, value] of Object.entries(metadata)) if (key !== "reason") appendFileSync(process.env.GITHUB_OUTPUT, `${key}=${value}\n`);
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Version ${version}\n\n${metadata.reason}\n\nCommit: \`${commit}\`\n`);
    console.log(metadata.reason);
  } else if (command === "publish") {
    if (!requested || process.env.GITHUB_ACTIONS !== "true") throw new Error("Publication is allowed only through the workflow on the default branch.");
    if (commit !== process.env.RELEASE_COMMIT) throw new Error("The checkout does not match the validated commit.");
    const range = previous ? `${previous}..${commit}` : commit;
    const commits = parseGitLog(git("log", "--no-merges", "--max-count=501", "--format=%H%x00%B%x00", range));
    const editorial = resolve("release-notes", `${version}.md`);
    const result = await publishRelease({ version, commit, previous, repository: process.env.GITHUB_REPOSITORY, directory: resolve(process.argv[3]), commits, curated: existsSync(editorial) ? readFileSync(editorial, "utf8") : "" }, api);
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Release\n\n[Cooldown Bar v${version}](${result.url})${result.alreadyPublished ? " was already published. No files were replaced." : " was published after all four files and their hashes were verified."}\n`);
    console.log(result.url);
  } else throw new Error("Usage: node scripts/ci/release.mjs prepare | publish <directory>");
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
