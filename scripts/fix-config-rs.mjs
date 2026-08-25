import fs from "fs";

let t = fs.readFileSync("src-tauri/src/config.rs", "utf8");

// Comment swallowed permission_mode onto same line
t = t.replace(
  /\/\/[^\n]*always-approve[^\n]*permission_mode: "ask"\.into\(\),/,
  `// Safe default: ask each time (D7). Settings can enable always-approve.
            permission_mode: "ask".into(),`
);

// Replace corrupted multi-byte log format strings with ASCII
t = t.replace(
  /tracing::info!\(\s*"[^"]*"\s*,\s*st\.projects\.len\(\),\s*path\.display\(\)\s*\);/g,
  `tracing::info!("loaded desktop state: {} projects · {}", st.projects.len(), path.display());`
);
t = t.replace(
  /tracing::error!\(\s*"[^"]*"\s*,\s*path\.display\(\),\s*e\s*\);/g,
  `tracing::error!("failed to parse desktop state ({}): {} — using empty state, original backed up", path.display(), e);`
);
t = t.replace(
  /tracing::warn!\(\s*"[^"]*"\s*,\s*path\.display\(\),\s*e\s*\);/g,
  `tracing::warn!("cannot read desktop state {}: {}", path.display(), e);`
);
t = t.replace(
  /tracing::info!\(\s*"[^"]*"\s*,\s*self\.projects\.len\(\),\s*path\.display\(\)\s*\);/g,
  `tracing::info!("saved desktop state: {} projects · {}", self.projects.len(), path.display());`
);

// Strip lines with replacement character in comments (broken encoding)
t = t
  .split("\n")
  .map((line) => {
    if (line.includes("\uFFFD") && line.trimStart().startsWith("//")) {
      return "            // (comment)";
    }
    if (line.includes("\uFFFD") && line.includes("tracing::")) {
      // already handled above ideally
      return line.replace(/\uFFFD/g, "?");
    }
    return line.replace(/\uFFFD/g, "?");
  })
  .join("\n");

// Ensure theme default is light
t = t.replace(/theme: "dark"\.into\(\)/g, 'theme: "light".into()');

fs.writeFileSync("src-tauri/src/config.rs", t, "utf8");
console.log("config.rs repaired");

// Validate braces roughly
let n = 0;
for (const ch of t) {
  if (ch === "{") n++;
  if (ch === "}") n--;
}
console.log("brace balance", n);
const i = t.indexOf("pub fn defaults");
console.log(t.slice(i, i + 450));
