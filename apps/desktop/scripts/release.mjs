// `pnpm release <patch|minor|major|x.y.z> [--no-build] [--expiring | --expires YYYY-MM-DD]`:
// the one way the version
// changes. It bumps the workspace version in the root Cargo.toml (the single source
// of truth; Cargo.lock follows), commits, tags `vX.Y.Z` and builds the installer,
// which then carries the plain version in its name (see build.mjs). If the build
// fails, the release commit and tag are taken back (nothing has been pushed yet), so
// the number stays free and the tree is as it was. Pushed tags are never moved.
import { execFileSync, spawnSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const cargoToml = join(root, "Cargo.toml");
const [bump, ...flags] = process.argv.slice(2);
// --expiring / --expires (and the old --no-expiry) go on to build.mjs, which checks the date.
const expiryFlags = [];
let unknown = false;
for (let i = 0; i < flags.length; i++) {
  if (flags[i] === "--expires") expiryFlags.push(flags[i], flags[++i] ?? "");
  else if (["--expiring", "--no-expiry"].includes(flags[i]) || flags[i].startsWith("--expires=")) expiryFlags.push(flags[i]);
  else if (flags[i] !== "--no-build") unknown = true;
}

if (!bump || !/^(patch|minor|major|\d+\.\d+\.\d+)$/.test(bump) || unknown) {
  console.error("usage: pnpm release <patch|minor|major|x.y.z> [--no-build] [--expiring | --expires YYYY-MM-DD]");
  process.exit(2);
}

const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] }).trim();
const fail = (msg) => {
  console.error(msg);
  process.exit(1);
};

// A release must be reproducible from its tag: nothing modified, nothing untracked.
const status = run("git", ["status", "--porcelain"]);
if (status) fail(`The working tree is not clean. Commit or stash first:\n${status}`);
const branch = run("git", ["branch", "--show-current"]);
if (branch !== "main") console.warn(`Note: releasing from branch '${branch}', not main.`);

const toml = readFileSync(cargoToml, "utf8");
const re = /(\[workspace\.package\][^[]*?^version = ")(\d+)\.(\d+)\.(\d+)(")/m;
const m = toml.match(re);
if (!m) fail("Could not find the [workspace.package] version in Cargo.toml");
const current = `${m[2]}.${m[3]}.${m[4]}`;
let next = bump;
if (!/^\d/.test(bump)) {
  let [major, minor, patch] = [m[2], m[3], m[4]].map(Number);
  if (bump === "major") [major, minor, patch] = [major + 1, 0, 0];
  else if (bump === "minor") [minor, patch] = [minor + 1, 0];
  else patch += 1;
  next = `${major}.${minor}.${patch}`;
}
if (next === current) fail(`The version is already ${current}.`);
if (run("git", ["tag", "--list", `v${next}`])) fail(`Tag v${next} already exists.`);

writeFileSync(cargoToml, toml.replace(re, `$1${next}$5`));
// Cargo.lock lists every workspace crate with its version.
const update = (extra) => spawnSync("cargo", ["update", "--workspace", ...extra], { cwd: root, stdio: "inherit" });
if (update(["--offline"]).status !== 0 && update([]).status !== 0) {
  writeFileSync(cargoToml, toml); // leave the tree as it was
  fail("cargo update --workspace failed; Cargo.toml was restored. Nothing was committed.");
}
const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
if (!new RegExp(`name = "dettino"\\r?\\nversion = "${next.replaceAll(".", "\\.")}"`).test(lock)) fail("Cargo.lock does not list dettino at the new version.");

run("git", ["add", "Cargo.toml", "Cargo.lock"]);
run("git", ["commit", "-q", "-m", `Release v${next}`]);
run("git", ["tag", "-a", `v${next}`, "-m", `Dettino ${next}`]);
console.log(`${current} -> ${next}: committed and tagged v${next}.`);

if (!flags.includes("--no-build")) {
  const r = spawnSync(process.execPath, [join(here, "build.mjs"), ...expiryFlags], { stdio: "inherit" });
  if (r.status !== 0) {
    // Undo only what this script did: the tag, the commit, and the two bumped files.
    run("git", ["tag", "-d", `v${next}`]);
    run("git", ["reset", "-q", "--soft", "HEAD~1"]);
    run("git", ["restore", "--staged", "--worktree", "--source=HEAD", "--", "Cargo.toml", "Cargo.lock"]);
    fail(`The build failed. The release commit and tag v${next} were taken back; fix the build and run the release again.`);
  }
}
console.log("Publish with: git push --follow-tags");
