// `pnpm build [--expiring | --expires YYYY-MM-DD]`: `tauri build` plus the per-OS
// bundle config that ships the native speech libraries, the build identity and the
// optional end date for src-tauri/build.rs, and the installer name. A build never
// expires unless asked to: `--expiring` ends it EXPIRY_DAYS after the build day,
// `--expires` on a given date (see src-tauri/src/expiry.rs). On macOS it also picks
// the signing identity: macOS ties the Microphone and
// Accessibility permissions to the signature, so an ad-hoc signed build loses them
// on every rebuild while a Developer ID signed one keeps them.
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, renameSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const root = resolve(appDir, "../..");
const os = { darwin: "macos", win32: "windows" }[process.platform] ?? "linux";
const config = join(appDir, "src-tauri", `tauri.bundle.${os}.conf.json`);
const EXPIRY_DAYS = 60;
const { expires, rest } = expiryArgs(process.argv.slice(2));
const args = ["tauri", "build", ...(existsSync(config) ? ["--config", config] : []), ...rest];
const env = { ...process.env };
if (expires) env.DETTINO_EXPIRES = expires;
else delete env.DETTINO_EXPIRES;

// Build identity (see the env contract in src-tauri/build.rs; same four variables,
// same git commands). The version is the workspace version in the root Cargo.toml,
// the one place it is defined.
const product = JSON.parse(readFileSync(join(appDir, "src-tauri", "tauri.conf.json"), "utf8")).productName;
const version = workspaceVersion();
const git = (...a) => {
  try {
    return execFileSync("git", a, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
  } catch {
    return null;
  }
};
const commit = git("rev-parse", "--short=9", "HEAD") ?? "unknown";
const dirty = (git("status", "--porcelain") ?? "") !== "";
const tagged = (git("tag", "--points-at", "HEAD") ?? "").split(/\r?\n/).includes(`v${version}`);
const commitDate = git("show", "-s", "--format=%cs", "HEAD") ?? "unknown";
Object.assign(env, {
  DETTINO_BUILD_COMMIT: commit,
  DETTINO_BUILD_DIRTY: dirty ? "1" : "0",
  DETTINO_BUILD_TAGGED: tagged ? "1" : "0",
  DETTINO_BUILD_COMMIT_DATE: commitDate,
});
// Mirrors `label_for` in src-tauri/src/build_info.rs: a clean build at its own tag
// is just the version; anything else names the commit, plus `.dirty` for changes.
const label = tagged && !dirty ? version : `${version}+${commit}${dirty ? ".dirty" : ""}`;
console.log(`Building ${product} ${label}${label === version ? " (release)" : ""}`);
console.log(expires ? `Works until ${expires} (${daysFromToday(expires)} days from today)` : "No end date (add --expiring for a test build that stops after 60 days)");

if (os === "macos" && !env.APPLE_SIGNING_IDENTITY) {
  let found = null;
  try {
    const ids = execFileSync("security", ["find-identity", "-v", "-p", "codesigning"], { encoding: "utf8" });
    found = ids.match(/"(Developer ID Application: [^"]+)"/)?.[1] ?? null;
  } catch {
    // no keychain access: fall through to ad-hoc
  }
  if (!found && env.CI) {
    console.error("No Developer ID certificate found and CI is set: refusing to ship an ad-hoc signed build.");
    process.exit(1);
  }
  env.APPLE_SIGNING_IDENTITY = found ?? "-";
  console.log(found ? `Signing as ${found}` : "No Developer ID certificate found: ad-hoc signing (permissions reset on every build)");
}

// On Windows pnpm is a .cmd, which needs a shell; a shell splits on spaces, so
// arguments with spaces (paths) must be quoted by hand.
const shell = process.platform === "win32";
const argv = shell ? args.map((a) => (/\s/.test(a) ? `"${a}"` : a)) : args;
const r = spawnSync("pnpm", argv, { cwd: appDir, stdio: "inherit", env, shell });
if (r.error) {
  console.error(`Could not run pnpm: ${r.error.message}`);
  process.exit(1);
}
if (r.status === 0) renameBundles();
process.exit(r.status ?? 1);

// Our own flags; everything else goes on to `tauri build`.
function expiryArgs(argv) {
  let expires = null;
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--expiring") expires = localDate(addDays(new Date(), EXPIRY_DAYS));
    else if (a === "--no-expiry") expires = null; // the default; still accepted
    else if (a === "--expires" || a.startsWith("--expires=")) {
      const d = a === "--expires" ? argv[++i] : a.slice("--expires=".length);
      if (!d || !/^\d{4}-\d{2}-\d{2}$/.test(d) || localDate(new Date(`${d}T12:00`)) !== d) {
        console.error(`--expires needs a real date as YYYY-MM-DD, got ${d || "nothing"}`);
        process.exit(2);
      }
      expires = d;
    } else rest.push(a);
  }
  return { expires, rest };
}

function addDays(d, n) {
  const r = new Date(d);
  r.setDate(r.getDate() + n);
  return r;
}

// YYYY-MM-DD in local time, the calendar the app compares against.
function localDate(d) {
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

function daysFromToday(d) {
  const today = new Date(`${localDate(new Date())}T12:00`);
  return Math.round((new Date(`${d}T12:00`) - today) / 86_400_000);
}

function workspaceVersion() {
  const toml = readFileSync(join(root, "Cargo.toml"), "utf8");
  const m = toml.match(/\[workspace\.package\][^[]*?^version = "(\d+\.\d+\.\d+)"/m);
  if (!m) {
    console.error("Could not read the [workspace.package] version from Cargo.toml");
    process.exit(1);
  }
  return m[1];
}

// Tauri names bundles `<product>_<version>_<arch>…`. A build that is not a clean
// build at its tag gets the label instead, so two installers of the same version
// can never be mistaken for one another. A build with an end date carries it too,
// e.g. `Dettino_0.2.0_until-2026-12-29_x64-setup.exe`. Tauri's own version stays
// plain semver: the NSIS VIProductVersion needs numeric parts, and the installer's
// upgrade detection compares versions.
function renameBundles() {
  const profile = args.includes("--debug") ? "debug" : "release";
  for (const kind of ["nsis", "dmg"]) {
    const dir = join(root, "target", profile, "bundle", kind);
    if (!existsSync(dir)) continue;
    for (const f of readdirSync(dir)) {
      if (!f.startsWith(`${product}_${version}_`)) continue;
      const tail = f.slice(`${product}_${version}_`.length);
      if (tail.startsWith("until-")) continue; // renamed by an earlier build
      const name = `${product}_${label}${expires ? `_until-${expires}` : ""}_${tail}`;
      const to = join(dir, name);
      if (name !== f) renameSync(join(dir, f), to);
      console.log(`Installer: ${to}`);
    }
  }
}
