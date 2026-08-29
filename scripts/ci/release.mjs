import { execFileSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { GitHub } from "./github.mjs";
import { assetNames, verifyArtifacts } from "./artifacts.mjs";
import { compareVersions, parseVersion, readVersions } from "./version.mjs";

const git = (...args) => execFileSync("git", args, { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 }).trim();
const escapeMarkdown = (text) => text.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/([\\`*_[\]<>])/g, "\\$1");
const marker = (commit) => `<!-- notchusage-release:${commit} -->`;

export function releaseRequested({ eventName, ref, defaultBranch, publish }) {
  const main = ref === `refs/heads/${defaultBranch}`;
  if (eventName === "workflow_dispatch" && publish && !main) throw new Error("Publicação manual permitida somente na branch padrão do repositório.");
  return main && (eventName === "push" || (eventName === "workflow_dispatch" && publish));
}

export function previousTag(tags, version) {
  return tags.filter((tag) => /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag))
    .filter((tag) => compareVersions(tag.slice(1), version) < 0)
    .sort((a, b) => compareVersions(b.slice(1), a.slice(1)))[0] ?? "";
}

export async function prepareRelease({ version, commit, requested, tags }, api) {
  parseVersion(version);
  if (!/^[a-f0-9]{40}$/.test(commit)) throw new Error("Commit inválido.");
  const tag = `v${version}`, previous = previousTag(tags, version);
  const metadata = { version, commit, tag, previous_tag: previous, publish: false };
  if (!requested) return { ...metadata, reason: "Validação sem publicação." };
  const release = await api.release(tag);
  // Normal pushes after a release keep validating the code without republishing it.
  if (release && !release.draft) return { ...metadata, reason: `${tag} já publicada; altere a versão para uma nova release.` };
  for (const other of tags) {
    if (/^v\d+\.\d+\.\d+$/.test(other) && compareVersions(other.slice(1), version) > 0) {
      throw new Error(`A versão ${version} é anterior à tag ${other}; não será publicada como latest.`);
    }
  }
  const target = await api.tagCommit(tag);
  if (target && target !== commit) throw new Error(`A tag ${tag} já aponta para outro commit. Aumente a versão; tags nunca são movidas.`);
  if (release && !release.body?.includes(marker(commit))) throw new Error("Existe um rascunho não gerenciado por este CI. Revise-o manualmente antes de continuar.");
  return { ...metadata, publish: true, reason: `Publicar ${tag} após todos os testes e o build.` };
}

export function parseGitLog(text) {
  const fields = text.split("\0"), commits = [];
  for (let i = 0; i + 1 < fields.length; i += 2) {
    const sha = fields[i].trim(), message = fields[i + 1].trim();
    if (!/^[a-f0-9]{40}$/.test(sha)) throw new Error("Histórico Git inválido.");
    const [subject, ...body] = message.split("\n");
    commits.push({ sha, subject, body: body.join("\n") });
  }
  return commits;
}

export function classifyCommit({ subject, body = "" }) {
  const match = subject.match(/^([a-z]+)(?:\(([^)]+)\))?(!)?:\s*(.+)$/i);
  const type = match?.[1]?.toLowerCase(), scope = match?.[2]?.toLowerCase();
  const breaking = Boolean(match?.[3]) || /^BREAKING[ -]CHANGE:/m.test(body);
  let category = "Outras mudanças";
  if (breaking) category = "Mudanças que exigem atenção";
  else if (type === "security" || scope === "security") category = "Segurança";
  else if (scope === "deps" || scope === "deps-dev" || type === "deps") category = "Dependências";
  else category = ({ feat: "Novidades", fix: "Correções", perf: "Desempenho", docs: "Documentação", ci: "Build e automação", build: "Build e automação", refactor: "Manutenção", test: "Testes", chore: "Manutenção" })[type] ?? category;
  return { category, breaking, title: match ? `${match[2] ? `${match[2]}: ` : ""}${match[4]}` : subject };
}

const categoryOrder = ["Mudanças que exigem atenção", "Segurança", "Novidades", "Correções", "Desempenho", "Dependências", "Build e automação", "Documentação", "Testes", "Manutenção", "Outras mudanças"];

export function renderNotes({ version, commit, previous, repository, commits, curated = "", generated = "", signing = "adhoc" }) {
  parseVersion(version);
  const repo = `https://github.com/${repository}`, tag = `v${version}`;
  const grouped = new Map();
  for (const entry of commits.slice(0, 200)) {
    const item = classifyCommit(entry);
    if (!grouped.has(item.category)) grouped.set(item.category, []);
    grouped.get(item.category).push(`- ${escapeMarkdown(item.title)} ([${entry.sha.slice(0, 7)}](${repo}/commit/${entry.sha}))`);
  }
  const historyLink = previous ? `${repo}/compare/${previous}...${tag}` : `${repo}/commits/${tag}`;
  const parts = [marker(commit), `# NotchUsage ${tag}`, previous ? `Atualização desde **${previous}**.` : "Primeira versão publicada do NotchUsage."];
  if (curated.trim()) parts.push("## Destaques", curated.trim());
  if (generated.trim()) parts.push("## Pull requests e colaboradores", generated.trim());
  parts.push("<details>\n<summary>Mudanças por commit</summary>\n");
  for (const category of categoryOrder) if (grouped.has(category)) parts.push(`### ${category}`, grouped.get(category).join("\n"));
  if (!commits.length) parts.push("Nenhum commit adicional no intervalo desta versão.");
  if (commits.length > 200) parts.push("Exibidos os primeiros 200 commits; consulte o histórico completo abaixo.");
  parts.push("</details>", `[Histórico completo](${historyLink})`);
  const names = assetNames(version);
  parts.push("## Download e instalação", "Compatível com **macOS 13 ou superior**, em **Apple Silicon e Intel**. O mesmo instalador atende às duas arquiteturas.",
    `- [Instalador DMG](${repo}/releases/download/${tag}/${names[0]}) — abra e arraste NotchUsage para Aplicativos.`,
    `- [Aplicativo em ZIP](${repo}/releases/download/${tag}/${names[1]}) — alternativa ao DMG.`,
    `- [SHA256SUMS.txt](${repo}/releases/download/${tag}/SHA256SUMS.txt) e [metadados do build](${repo}/releases/download/${tag}/build-info.json).`,
    signing === "developer-id-notarized"
      ? "**Assinatura:** Developer ID; notarização e ticket Apple verificados pelo CI."
      : "**Build de teste com assinatura ad hoc, sem notarização Apple.** O Gatekeeper pode bloquear a abertura. Siga as opções de Privacidade e Segurança do macOS somente se confiar na origem. Esta assinatura não comprova a identidade de um desenvolvedor Apple.",
    "Para conferir a integridade, baixe os dois pacotes, `build-info.json` e `SHA256SUMS.txt` para a mesma pasta e execute:\n\n```bash\nshasum -a 256 -c SHA256SUMS.txt\n```",
    `Commit compilado: [\`${commit}\`](${repo}/commit/${commit}).`,
    "As categorias são inferidas das mensagens dos commits e dos labels dos pull requests; não representam uma análise automática do comportamento do código.");
  const notes = `${parts.join("\n\n")}\n`;
  if (Buffer.byteLength(notes) > 120_000) throw new Error("Release notes muito extensas; reduza o resumo editorial.");
  return notes;
}

export async function publishRelease({ version, commit, previous, repository, directory, commits, curated }, api) {
  // No remote mutations occur before all artifacts and provenance are verified.
  const { assets, info } = verifyArtifacts(directory, { version, commit });
  const tag = `v${version}`;
  let release = await api.release(tag);
  const target = await api.tagCommit(tag);
  if (target && target !== commit) throw new Error(`A tag ${tag} aponta para outro commit; publicação recusada.`);
  if (release && !release.draft) {
    if (target !== commit) throw new Error("A release existente não corresponde ao commit compilado.");
    return { url: release.html_url, alreadyPublished: true };
  }
  if (release && !release.body?.includes(marker(commit))) throw new Error("O rascunho existente não pertence a esta execução/commit.");
  // A failed old run can be retried after a newer version has already shipped.
  // Recheck remotely here, even when the preparation job was not rerun.
  const latest = await api.request("GET", "/releases/latest", undefined, { allow404: true });
  if (latest?.tag_name && /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(latest.tag_name) && compareVersions(latest.tag_name.slice(1), version) > 0) {
    throw new Error(`Uma versão mais recente (${latest.tag_name}) já foi publicada; esta execução antiga não será promovida a latest.`);
  }
  let generated = "";
  try {
    const response = await api.request("POST", "/releases/generate-notes", {
      tag_name: tag, target_commitish: commit, configuration_file_path: ".github/release.yml",
      ...(previous ? { previous_tag_name: previous } : {}),
    });
    generated = response.body;
  } catch {
    console.warn("Notas de PRs indisponíveis; usando os commits locais e o resumo editorial.");
  }
  const body = renderNotes({ version, commit, previous, repository, commits, curated, generated, signing: info.signing });
  if (!target) await api.request("POST", "/git/refs", { ref: `refs/tags/${tag}`, sha: commit });
  if (!release) release = await api.request("POST", "/releases", { tag_name: tag, target_commitish: commit, name: `NotchUsage ${tag}`, body, draft: true, prerelease: false });
  else release = await api.request("PATCH", `/releases/${release.id}`, { body, name: `NotchUsage ${tag}` });

  const existing = await api.assets(release.id);
  if (existing.some((a) => !assets.some((expected) => a.name === expected.name))) throw new Error("O rascunho contém arquivos adicionais; revisão manual necessária.");
  for (const asset of assets) {
    const old = existing.find((a) => a.name === asset.name);
    if (old?.state === "uploaded" && old.digest === asset.digest && old.size === asset.size) continue;
    // Only this CI's draft can be repaired. Published assets are never replaced.
    if (old) await api.request("DELETE", `/releases/assets/${old.id}`);
    await api.request("POST", `/releases/${release.id}/assets?name=${encodeURIComponent(asset.name)}`, asset.data, { binary: true });
  }
  const uploaded = await api.assets(release.id);
  if (uploaded.length !== assets.length || assets.some((expected) => !uploaded.some((actual) => actual.name === expected.name && actual.size === expected.size && actual.digest === expected.digest && actual.state === "uploaded"))) {
    throw new Error("Verificação dos uploads falhou. A release permanece em rascunho.");
  }
  if (await api.tagCommit(tag) !== commit) throw new Error("A tag mudou durante a publicação; rascunho preservado.");
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
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Versão ${version}\n\n${metadata.reason}\n\nCommit: \`${commit}\`\n`);
    console.log(metadata.reason);
  } else if (command === "publish") {
    if (!requested || process.env.GITHUB_ACTIONS !== "true") throw new Error("Publicação permitida somente pelo workflow na branch padrão.");
    if (commit !== process.env.RELEASE_COMMIT) throw new Error("Checkout diferente do commit validado.");
    const range = previous ? `${previous}..${commit}` : commit;
    const commits = parseGitLog(git("log", "--no-merges", "--max-count=501", "--format=%H%x00%B%x00", range));
    const editorial = resolve("release-notes", `${version}.md`);
    const result = await publishRelease({ version, commit, previous, repository: process.env.GITHUB_REPOSITORY, directory: resolve(process.argv[3]), commits, curated: existsSync(editorial) ? readFileSync(editorial, "utf8") : "" }, api);
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Release\n\n[NotchUsage v${version}](${result.url})${result.alreadyPublished ? " já estava publicada; nenhum arquivo foi substituído." : " publicada após verificar os quatro arquivos e seus hashes."}\n`);
    console.log(result.url);
  } else throw new Error("Uso: node scripts/ci/release.mjs prepare | publish <diretório>");
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1; });
}
