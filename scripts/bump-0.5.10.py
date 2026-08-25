from pathlib import Path

pairs = [
    ("package.json", '"version": "0.5.9"', '"version": "0.5.10"'),
    ("src-tauri/tauri.conf.json", '"version": "0.5.9"', '"version": "0.5.10"'),
    ("src-tauri/Cargo.toml", 'version = "0.5.9"', 'version = "0.5.10"'),
    ("src/App.tsx", 'const APP_VERSION = "0.5.9"', 'const APP_VERSION = "0.5.10"'),
]
for rel, a, b in pairs:
    p = Path(rel)
    t = p.read_text(encoding="utf-8")
    if a not in t:
        print("MISS", rel)
    else:
        p.write_text(t.replace(a, b), encoding="utf-8")
        print("OK", rel)
sp = Path("src/screens/Settings.tsx")
st = sp.read_text(encoding="utf-8")
if "0.5.9" in st:
    sp.write_text(st.replace("0.5.9", "0.5.10"), encoding="utf-8")
    print("OK Settings")
