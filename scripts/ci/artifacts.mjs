import { createHash } from "node:crypto";
import { lstatSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { parseVersion } from "./version.mjs";

export const sha256 = (data) => createHash("sha256").update(data).digest("hex");
export function assetNames(version) {
  parseVersion(version);
  return [`Cooldown_Bar_${version}_universal.dmg`, `Cooldown_Bar_${version}_universal.app.zip`, "build-info.json", "SHA256SUMS.txt"];
}

export function writeManifest(directory, metadata) {
  const [dmg, app, manifest, sums] = assetNames(metadata.version);
  const files = Object.fromEntries([dmg, app].map((name) => {
    const data = readFileSync(join(directory, name));
    return [name, { bytes: data.length, sha256: sha256(data) }];
  }));
  writeFileSync(join(directory, manifest), `${JSON.stringify({ schemaVersion: 1, ...metadata, files }, null, 2)}\n`);
  writeFileSync(join(directory, sums), [dmg, app, manifest].map((name) => `${sha256(readFileSync(join(directory, name)))}  ${name}\n`).join(""));
}

export function verifyArtifacts(directory, { version, commit, minimumMacOS = "13.0" }) {
  const expected = assetNames(version);
  const actual = readdirSync(directory).sort();
  if (JSON.stringify(actual) !== JSON.stringify([...expected].sort())) throw new Error("Missing or unexpected files in the release package.");
  const assets = expected.map((name) => {
    const path = join(directory, name);
    const stat = lstatSync(path);
    if (!stat.isFile() || !stat.size) throw new Error(`Invalid file: ${name}`);
    const data = readFileSync(path);
    return { name, data, size: data.length, digest: `sha256:${sha256(data)}` };
  });
  const info = JSON.parse(assets[2].data.toString("utf8"));
  if (info.schemaVersion !== 1 || info.version !== version || info.commit !== commit || info.tag !== `v${version}` ||
      info.minimumMacOS !== minimumMacOS || JSON.stringify(info.architectures) !== JSON.stringify(["arm64", "x86_64"]) ||
      !["adhoc", "developer-id-notarized"].includes(info.signing)) {
    throw new Error("The package does not match the expected version, commit, platform, or signing mode.");
  }
  for (const asset of assets.slice(0, 2)) {
    if (info.files?.[asset.name]?.sha256 !== asset.digest.slice(7) || info.files[asset.name].bytes !== asset.size) {
      throw new Error(`Invalid integrity metadata: ${asset.name}`);
    }
  }
  const sums = assets.slice(0, 3).map((a) => `${a.digest.slice(7)}  ${a.name}\n`).join("");
  if (assets[3].data.toString("utf8") !== sums) throw new Error("SHA256SUMS.txt does not match the files.");
  return { info, assets };
}
