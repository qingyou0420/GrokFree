/**
 * Sync version across package.json, Cargo.toml, tauri.conf.json, App.tsx, Settings fallback.
 * Usage: node scripts/bump-version.mjs 0.5.13
 *
 * 发版验证门（本仓库无 git，pre-push hook 落不了地，改挂在 bump 前）：
 * typecheck + vitest + cargo test --lib。跳过：GROK_SKIP_VERIFY=1。
 */
import fs from "fs";
import path from "path";
import { spawnSync } from "child_process";
import { fileURLToPath } from "url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const ver = (process.argv[2] || "").trim();
if (!/^\d+\.\d+\.\d+/.test(ver)) {
  console.error("Usage: node scripts/bump-version.mjs <semver>");
  process.exit(1);
}

if (!process.env.GROK_SKIP_VERIFY) {
  const run = (cmd, args, opts = {}) => {
    console.log(`\n> ${cmd} ${args.join(" ")}`);
    const r = spawnSync(cmd, args, {
      stdio: "inherit",
      cwd: root,
      shell: process.platform === "win32",
      ...opts,
    });
    if (r.status !== 0) {
      console.error(`验证失败：${cmd} ${args.join(" ")}（退出码 ${r.status}）`);
      process.exit(1);
    }
  };
  console.log("== 发版前验证（GROK_SKIP_VERIFY=1 跳过）==");
  run("npm", ["run", "typecheck"]);
  run("npm", ["run", "test"]);
  run("cargo", ["test", "--lib"], { cwd: path.join(root, "src-tauri") });
  console.log("== 验证通过，开始 bump ==\n");
}

function read(rel) {
  return fs.readFileSync(path.join(root, rel), "utf8");
}
function write(rel, s) {
  fs.writeFileSync(path.join(root, rel), s, "utf8");
  console.log("updated", rel);
}

// package.json
{
  const p = "package.json";
  const j = JSON.parse(read(p));
  j.version = ver;
  write(p, JSON.stringify(j, null, 2) + "\n");
}

// Cargo.toml — only package version line
{
  const p = "src-tauri/Cargo.toml";
  let s = read(p);
  s = s.replace(/^version = "[^"]+"/m, `version = "${ver}"`);
  write(p, s);
}

// tauri.conf.json
{
  const p = "src-tauri/tauri.conf.json";
  const j = JSON.parse(read(p));
  j.version = ver;
  write(p, JSON.stringify(j, null, 2) + "\n");
}

// App.tsx APP_VERSION
{
  const p = "src/App.tsx";
  let s = read(p);
  s = s.replace(
    /const APP_VERSION = "[^"]+"/,
    `const APP_VERSION = "${ver}"`
  );
  write(p, s);
}

// Settings fallback version strings (optional)
{
  const p = "src/screens/Settings.tsx";
  let s = read(p);
  s = s.replace(/appInfo\?\.version \|\| "[^"]+"/g, `appInfo?.version || "${ver}"`);
  s = s.replace(/v\d+\.\d+\.\d+ ·/g, `v${ver} ·`);
  write(p, s);
}

console.log("bumped to", ver);
