import fs from "fs";

const appPath = "src/App.tsx";
const fragPath = "scripts/app-main-jsx.txt";
let s = fs.readFileSync(appPath, "utf8");
const frag = fs.readFileSync(fragPath, "utf8");

const startMarker = '    <div className={`app ${showReview ? "with-review" : ""}`}>';
const endMarker = "      {showSettings && state && (";
const start = s.indexOf(startMarker);
const end = s.indexOf(endMarker);
if (start < 0 || end < 0) {
  console.error("markers not found", { start, end });
  process.exit(1);
}
const returnIdx = s.lastIndexOf("  return (", start);
if (returnIdx < 0) {
  console.error("return not found");
  process.exit(1);
}

// frag ends with ConfirmDialog + blank line; next is showSettings
const out = s.slice(0, returnIdx) + frag + s.slice(end);
fs.writeFileSync(appPath, out);
console.log("OK App.tsx", out.length, "bytes; replaced", end - returnIdx, "->", frag.length);
