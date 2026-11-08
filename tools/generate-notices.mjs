// Run from the repository root after producing locked, Windows-filtered cargo metadata.
// Missing upstream notices are fetched only at the revisions recorded in published crates.
import fs from "node:fs/promises";
import path from "node:path";
import {createHash} from "node:crypto";

const metadataPath = process.argv[2];
if (!metadataPath || process.argv.length !== 3) throw Error("Usage: node tools/generate-notices.mjs <cargo-metadata.json>");
const metadata = JSON.parse(await fs.readFile(metadataPath, "utf8"));
const lock = JSON.parse(await fs.readFile("package-lock.json", "utf8"));
const sha = text => createHash("sha256").update(text).digest("hex");
const texts = new Map();
const packages = [];

function retain(text) {
    const normalized = text.replaceAll("\r\n", "\n").trim();
    if (!normalized) throw Error("Empty license notice");
    const digest = sha(normalized);
    if (!texts.has(digest)) texts.set(digest, {id: `N${String(texts.size + 1).padStart(3, "0")}`, text: normalized});
    return texts.get(digest).id;
}

async function localNotices(directory) {
    const result = [];
    for (const entry of (await fs.readdir(directory, {withFileTypes: true})).sort((a, b) => a.name.localeCompare(b.name))) {
        if (!/^(licen[cs]e|copying|copyright|notice)([-_.]|$)/i.test(entry.name)) continue;
        if (entry.isFile()) result.push({name: entry.name, text: await fs.readFile(path.join(directory, entry.name), "utf8")});
    }
    return result;
}

async function crateNotices(pkg) {
    const directory = path.dirname(pkg.manifest_path);
    const local = await localNotices(directory);
    if (local.length) return local;
    if (pkg.name === "selectors" && pkg.version === "0.38.0") {
        // This crate includes the MPL reference in its source but no copy of the license.
        const source = await fs.readFile(path.join(directory, "lib.rs"), "utf8");
        if (!source.startsWith("/* This Source Code Form is subject to the terms of the Mozilla Public")) throw Error("Selectors license header changed");
        const mpl = metadata.packages.find(item => item.name === "cssparser" && item.version === "0.37.0" && item.license === "MPL-2.0");
        if (!mpl) throw Error("Reviewed MPL text unavailable");
        return [{name: "MPL-2.0 (identical license text distributed by cssparser 0.37.0)", text: await fs.readFile(path.join(path.dirname(mpl.manifest_path), "LICENSE"), "utf8")}];
    }
    const missing = {
        "alloc-stdlib@0.3.0": ["dropbox/rust-alloc-no-stdlib", ["LICENSE"]],
        "defmt-parser@1.0.0": ["knurling-rs/defmt", ["LICENSE-MIT", "LICENSE-APACHE"]],
        "tauri-plugin@2.7.2": ["tauri-apps/tauri", ["LICENSE-MIT", "LICENSE-APACHE-2.0"]],
        "webview2-com@0.39.1": ["wravery/webview2-rs", ["LICENSE"]],
        "webview2-com-macros@0.8.1": ["wravery/webview2-rs", ["LICENSE"]],
        "webview2-com-sys@0.39.1": ["wravery/webview2-rs", ["LICENSE"]]
    }[`${pkg.name}@${pkg.version}`];
    if (!missing) throw Error(`Review missing notices: ${pkg.name}@${pkg.version}`);
    const [repository, names] = missing;
    if (pkg.repository !== `https://github.com/${repository}`) throw Error("Upstream repository changed");
    const vcs = JSON.parse(await fs.readFile(path.join(directory, ".cargo_vcs_info.json"), "utf8"));
    if (!/^[a-f0-9]{40}$/.test(vcs.git.sha1)) throw Error("Upstream revision unavailable");
    return Promise.all(names.map(async name => {
        const url = `https://raw.githubusercontent.com/${repository}/${vcs.git.sha1}/${name}`;
        const response = await fetch(url, {signal: AbortSignal.timeout(30000)});
        if (!response.ok) throw Error(`License download failed: ${pkg.name}/${name} (${response.status})`);
        return {name: url, text: await response.text()};
    }));
}

for (const pkg of metadata.packages.filter(item => item.source).sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`))) {
    if (!pkg.license || !pkg.source.startsWith("registry+")) throw Error(`Review dependency source/license: ${pkg.name}`);
    const notices = await crateNotices(pkg);
    packages.push({name: `${pkg.name} ${pkg.version}`, license: pkg.license,
        source: `https://crates.io/crates/${pkg.name}/${pkg.version}`, notices: notices.map(item => ({source: item.name, id: retain(item.text)}))});
}
// The Rust wrapper's MIT notice does not cover the Microsoft loader it links on MSVC.
const webview = metadata.packages.find(pkg => pkg.name === "webview2-com-sys");
if (!webview || webview.version !== "0.39.1") throw Error("Review the bundled WebView2 loader version");
const loader = await fs.readFile(path.join(path.dirname(webview.manifest_path), "x64/WebView2LoaderStatic.lib"));
if (sha(loader) !== "89c6b872783b8f6c3cedbff618adb42082d455c615453ae10cfc753f1e8f25d8") throw Error("Bundled WebView2 loader changed");
const loaderNotice = "tools/licenses/webview2-loader-1.0.3800.47.txt";
const loaderLicense = (await fs.readFile(loaderNotice, "utf8")).replaceAll("\r\n", "\n");
if (sha(loaderLicense) !== "0af8f1b807512aae39c2ac1aa4d0cae65cabecb6fd554b8439a5162a0d6eca55") throw Error("Microsoft SDK notice changed");
packages.push({name: "Microsoft WebView2 SDK loader 1.0.3800.47", license: "BSD-3-Clause",
    source: "https://www.nuget.org/packages/Microsoft.Web.WebView2/1.0.3800.47",
    notices: [{source: loaderNotice, id: retain(loaderLicense)}]});

for (const [directory, pkg] of Object.entries(lock.packages).filter(([directory, pkg]) => directory && !pkg.dev).sort()) {
    const installed = JSON.parse(await fs.readFile(path.join(directory, "package.json"), "utf8"));
    if (installed.version !== pkg.version || !installed.license) throw Error(`Dependency version/license mismatch: ${directory}`);
    const notices = await localNotices(directory);
    if (!notices.length) throw Error(`Review missing notices: ${installed.name}`);
    packages.push({name: `${installed.name} ${pkg.version}`, license: installed.license,
        source: `https://www.npmjs.com/package/${installed.name}/v/${pkg.version}`, notices: notices.map(item => ({source: item.name, id: retain(item.text)}))});
}

const heading = `Jentrics - third-party notices

Generated from locked Rust dependencies for Windows (including build/dev dependencies)
and production npm dependencies. This inventory is deliberately broader than the
code included in the installer; it is not a runtime software-component assertion.
Dependencies are unmodified. Exact source releases are linked below; Cargo.lock
and package-lock.json identify the complete dependency versions and checksums.
The Microsoft WebView2 SDK loader notice is included separately. WebView2's
browser runtime is separately installed and is not relicensed by Jentrics.

Cargo.lock SHA-256 (LF-normalized): ${sha((await fs.readFile("src-tauri/Cargo.lock", "utf8")).replaceAll("\r\n", "\n"))}
package-lock.json SHA-256 (LF-normalized): ${sha((await fs.readFile("package-lock.json", "utf8")).replaceAll("\r\n", "\n"))}

Identical license/notice texts are stored once and referenced by notice ID.
For MPL-2.0 packages, the linked source release provides the corresponding source.
Regenerate with tools/generate-notices.mjs; see README.md.
`;
const entries = packages.map(pkg => `\n${pkg.name}\nDeclared license: ${pkg.license}\nSource: ${pkg.source}\n${pkg.notices.map(item => `Notice ${item.id}: ${item.source}`).join("\n")}\n`).join("");
const appendix = [...texts.values()].map(({id, text}) => `\n${"=".repeat(72)}\nNotice ${id}\n${"=".repeat(72)}\n\n${text}\n`).join("");
await fs.writeFile("THIRD_PARTY_NOTICES.txt", heading + entries + appendix);
console.log(JSON.stringify({packages: packages.length, distinctNotices: texts.size, output: "THIRD_PARTY_NOTICES.txt"}));
