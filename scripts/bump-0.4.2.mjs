import fs from "fs";

const ver = "0.4.2";

const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
pkg.version = ver;
fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");

let cargo = fs.readFileSync("src-tauri/Cargo.toml", "utf8");
cargo = cargo.replace(/^version = ".*"/m, `version = "${ver}"`);
fs.writeFileSync("src-tauri/Cargo.toml", cargo);

const conf = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8"));
conf.version = ver;
fs.writeFileSync("src-tauri/tauri.conf.json", JSON.stringify(conf, null, 2) + "\n");

let app = fs.readFileSync("src/App.tsx", "utf8");
app = app.replace(/const APP_VERSION = "[^"]+"/, `const APP_VERSION = "${ver}"`);
fs.writeFileSync("src/App.tsx", app);

// Settings fallback version string
let settings = fs.readFileSync("src/screens/Settings.tsx", "utf8");
settings = settings.replace(/appInfo\?\.version \|\| "[^"]+"/, `appInfo?.version || "${ver}"`);
settings = settings.replace(/v0\.4\.\d+ ·/, `v${ver} ·`);
fs.writeFileSync("src/screens/Settings.tsx", settings);

console.log("bumped to", ver);
