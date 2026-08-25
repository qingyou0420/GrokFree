from pathlib import Path

files = {
    "package.json": ('"version": "0.5.7"', '"version": "0.5.8"'),
    "src-tauri/tauri.conf.json": ('"version": "0.5.7"', '"version": "0.5.8"'),
    "src-tauri/Cargo.toml": ('version = "0.5.7"', 'version = "0.5.8"'),
    "src/App.tsx": (
        'const APP_VERSION = "0.5.7"',
        'const APP_VERSION = "0.5.8"',
    ),
}

for rel, (a, b) in files.items():
    p = Path(rel)
    t = p.read_text(encoding="utf-8")
    if a not in t:
        print("MISS", rel, a)
        continue
    p.write_text(t.replace(a, b), encoding="utf-8")
    print("OK", rel)

# Settings fallback strings
sp = Path("src/screens/Settings.tsx")
st = sp.read_text(encoding="utf-8")
st2 = st.replace("0.5.7", "0.5.8")
if st2 != st:
    sp.write_text(st2, encoding="utf-8")
    print("OK Settings.tsx")
else:
    print("Settings already or no 0.5.7")
