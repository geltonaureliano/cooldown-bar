import { execFileSync, spawnSync } from "node:child_process";
import { appendFileSync, copyFileSync, mkdirSync, readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { assetNames, verifyArtifacts, writeManifest } from "./artifacts.mjs";
import { readVersions } from "./version.mjs";

export const appleSecrets = ["APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_SIGNING_IDENTITY", "APPLE_ID", "APPLE_PASSWORD", "APPLE_TEAM_ID"];

export function buildEnvironment(environment) {
  const env = { ...environment };
  const signed = env.SIGNING_ENABLED === "true";
  if (signed) {
    const missing = appleSecrets.filter((key) => !env[key]?.trim());
    if (missing.length) throw new Error(`Assinatura ativada, mas faltam secrets: ${missing.join(", ")}. Não haverá fallback silencioso.`);
    if (!env.APPLE_SIGNING_IDENTITY.startsWith("Developer ID Application:")) throw new Error("Use um certificado Developer ID Application para distribuição fora da App Store.");
  } else {
    // Empty env variables can still activate Tauri's signing/notarization paths.
    for (const key of appleSecrets) delete env[key];
    env.APPLE_SIGNING_IDENTITY = "-";
  }
  // This workflow uses Apple ID + an app-specific password, not API key auth.
  for (const key of ["APPLE_API_ISSUER", "APPLE_API_KEY", "APPLE_API_KEY_PATH"]) delete env[key];
  return { env, signing: signed ? "developer-id-notarized" : "adhoc" };
}

function run(command, args, env = process.env) {
  const result = spawnSync(command, args, { env, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} falhou (${result.signal ?? result.status}).`);
}
const output = (command, args) => execFileSync(command, args, { encoding: "utf8", timeout: 120_000 }).trim();

export function packageMacOS(directory, signing, env = process.env) {
  const version = readVersions(), commit = output("git", ["rev-parse", "HEAD"]);
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
  const bundle = join(resolve(env.CARGO_TARGET_DIR ?? "src-tauri/target"), "universal-apple-darwin/release/bundle");
  const app = join(bundle, "macos/NotchUsage.app"), plist = join(app, "Contents/Info.plist");
  const plistValue = (key) => output("plutil", ["-extract", key, "raw", "-o", "-", plist]);
  if (plistValue("CFBundleShortVersionString") !== version) throw new Error("A versão do aplicativo compilado está incorreta.");
  if (plistValue("LSMinimumSystemVersion") !== config.bundle.macOS.minimumSystemVersion) throw new Error("Versão mínima do macOS incorreta no aplicativo.");
  const binary = join(app, "Contents/MacOS", plistValue("CFBundleExecutable"));
  const architectures = output("lipo", ["-archs", binary]).split(/\s+/).sort();
  if (JSON.stringify(architectures) !== JSON.stringify(["arm64", "x86_64"])) throw new Error("O aplicativo não é universal (Intel + Apple Silicon).");
  run("codesign", ["--verify", "--deep", "--strict", "--verbose=2", app]);
  if (signing === "developer-id-notarized") {
    run("xcrun", ["stapler", "validate", app]);
    run("spctl", ["--assess", "--type", "execute", "--verbose=2", app]);
  }
  const dmgs = readdirSync(join(bundle, "dmg")).filter((name) => name.endsWith(".dmg"));
  if (dmgs.length !== 1) throw new Error("Esperado exatamente um DMG no build limpo.");
  const dmg = join(bundle, "dmg", dmgs[0]);
  run("hdiutil", ["verify", dmg]);
  mkdirSync(directory, { recursive: true });
  if (readdirSync(directory).length) throw new Error("O diretório de distribuição deve estar vazio.");
  const [dmgName, appName] = assetNames(version);
  copyFileSync(dmg, join(directory, dmgName));
  // ditto preserves the bundle's permissions, symlinks and macOS metadata.
  run("ditto", ["-c", "-k", "--sequesterRsrc", "--keepParent", app, join(directory, appName)]);
  writeManifest(directory, { version, tag: `v${version}`, commit, architectures, minimumMacOS: config.bundle.macOS.minimumSystemVersion, signing });
  verifyArtifacts(directory, { version, commit, minimumMacOS: config.bundle.macOS.minimumSystemVersion });
  if (env.GITHUB_STEP_SUMMARY) appendFileSync(env.GITHUB_STEP_SUMMARY, `## Pacote macOS\n\n- Versão: **${version}**\n- Arquiteturas verificadas: Intel + Apple Silicon\n- Assinatura: **${signing}**\n- DMG, ZIP do aplicativo, metadados e SHA-256 verificados\n`);
}

function main() {
  if (process.platform !== "darwin") throw new Error("O empacotamento precisa executar no macOS.");
  if (process.argv[2] !== "build" || !process.argv[3]) throw new Error("Uso: node scripts/ci/macos.mjs build <diretório de distribuição vazio>");
  const { env, signing } = buildEnvironment(process.env);
  const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
  env.MACOSX_DEPLOYMENT_TARGET = config.bundle.macOS.minimumSystemVersion;
  env.CI = "true";
  console.log(`Build universal macOS; assinatura: ${signing}.`);
  run("npm", ["run", "tauri", "build", "--", "--ci", "--target", "universal-apple-darwin", "--bundles", "app,dmg", "--", "--locked"], env);
  packageMacOS(resolve(process.argv[3]), signing, env);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { main(); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
