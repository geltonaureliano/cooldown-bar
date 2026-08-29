import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

// Stable versions only: these also work as CFBundleShortVersionString on macOS.
export function parseVersion(value) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value ?? "")) {
    throw new Error(`Versão inválida: ${value}. Use MAJOR.MINOR.PATCH, sem prefixo v.`);
  }
  const parts = value.split(".").map(Number);
  if (parts.some((n) => !Number.isSafeInteger(n))) throw new Error("Versão fora do limite numérico.");
  return parts;
}

export function compareVersions(a, b) {
  const left = parseVersion(a), right = parseVersion(b);
  for (let i = 0; i < 3; i++) if (left[i] !== right[i]) return left[i] > right[i] ? 1 : -1;
  return 0;
}

export function nextVersion(current, requested) {
  const parts = parseVersion(current);
  const index = ["major", "minor", "patch"].indexOf(requested);
  let next = requested;
  if (index >= 0) {
    parts[index]++;
    for (let i = index + 1; i < 3; i++) parts[i] = 0;
    next = parts.join(".");
  }
  parseVersion(next);
  if (compareVersions(next, current) <= 0) throw new Error("A nova versão precisa ser maior que a atual.");
  return next;
}

export const versionFiles = [
  "package.json", "package-lock.json", "src-tauri/tauri.conf.json",
  "src-tauri/Cargo.toml", "src-tauri/Cargo.lock",
];

function rustVersionPattern(file) {
  return file.endsWith("Cargo.toml")
    ? /(\[package\][\s\S]*?\nversion\s*=\s*")([^"]+)(")/
    : /(\[\[package\]\]\nname = "notchusage"\nversion = ")([^"]+)(")/;
}

export function readVersions(root = process.cwd()) {
  const versions = {};
  for (const file of versionFiles) {
    const text = readFileSync(resolve(root, file), "utf8");
    if (file.endsWith(".json")) {
      const data = JSON.parse(text);
      versions[file] = data.version;
      if (file === "package-lock.json") versions[`${file} (root package)`] = data.packages?.[""]?.version;
    } else {
      versions[file] = text.match(rustVersionPattern(file))?.[2];
    }
  }
  for (const [file, version] of Object.entries(versions)) {
    try { parseVersion(version); } catch { throw new Error(`Versão ausente ou inválida em ${file}.`); }
  }
  if (new Set(Object.values(versions)).size !== 1) {
    throw new Error(`Versões divergentes. Use npm run release:version -- <versão>.\n${JSON.stringify(versions, null, 2)}`);
  }
  return versions["package.json"];
}

export function setVersion(requested, root = process.cwd()) {
  const current = readVersions(root);
  const version = nextVersion(current, requested);
  // Read/validate everything before writing; never touch dependency versions.
  const changes = versionFiles.map((file) => {
    const path = resolve(root, file), text = readFileSync(path, "utf8");
    if (!file.endsWith(".json")) return [path, text.replace(rustVersionPattern(file), `$1${version}$3`)];
    const data = JSON.parse(text);
    data.version = version;
    if (file === "package-lock.json") data.packages[""].version = version;
    return [path, `${JSON.stringify(data, null, 2)}\n`];
  });
  for (const [path, text] of changes) writeFileSync(path, text);
  readVersions(root);
  return version;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const command = process.argv[2];
    if (command === "check") console.log(`Versões sincronizadas: ${readVersions()}`);
    else if (command === "set" && process.argv.length === 4) console.log(`Versão atualizada: ${setVersion(process.argv[3])}. Revise e faça commit dos cinco arquivos.`);
    else throw new Error("Uso: node scripts/ci/version.mjs check | set <patch|minor|major|0.0.2>");
  } catch (error) { console.error(error.message); process.exitCode = 1; }
}
