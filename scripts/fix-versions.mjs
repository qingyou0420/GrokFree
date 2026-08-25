import fs from "fs";

const conf = {
  $schema: "https://schema.tauri.app/config/2",
  productName: "Grok Build Desktop",
  version: "0.4.1",
  identifier: "ai.x.grok.build.desktop",
  build: {
    beforeDevCommand: "npm run dev",
    devUrl: "http://localhost:1420",
    beforeBuildCommand: "npm run build",
    frontendDist: "../dist",
  },
  app: {
    windows: [
      {
        title: "Grok Build Desktop",
        width: 1440,
        height: 900,
        minWidth: 960,
        minHeight: 640,
        resizable: true,
        fullscreen: false,
        decorations: true,
        backgroundColor: "#f5f7fb",
      },
    ],
    security: {
      csp: "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self' ipc: http://ipc.localhost",
    },
  },
  bundle: {
    active: true,
    targets: ["nsis"],
    icon: [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico",
    ],
    windows: {
      nsis: {
        installMode: "currentUser",
        displayLanguageSelector: false,
      },
    },
  },
  plugins: {},
};

// Preserve $schema key
const confText =
  JSON.stringify(
    {
      $schema: conf.$schema,
      productName: conf.productName,
      version: conf.version,
      identifier: conf.identifier,
      build: conf.build,
      app: conf.app,
      bundle: conf.bundle,
      plugins: conf.plugins,
    },
    null,
    2
  ) + "\n";

fs.writeFileSync("src-tauri/tauri.conf.json", confText, "utf8");

const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
pkg.version = "0.4.1";
fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n", "utf8");

let cargo = fs.readFileSync("src-tauri/Cargo.toml", "utf8");
cargo = cargo.replace(/^version = ".*"/m, 'version = "0.4.1"');
fs.writeFileSync("src-tauri/Cargo.toml", cargo, "utf8");

let config = fs.readFileSync("src-tauri/src/config.rs", "utf8");
config = config.replace(/theme: "dark"\.into\(\)/, 'theme: "light".into()');
fs.writeFileSync("src-tauri/src/config.rs", config, "utf8");

console.log("fixed versions + utf8");
console.log("theme default:", /theme: "(light|dark)"/.exec(config)?.[1]);
