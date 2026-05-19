import { copyFile, mkdir, readdir, stat } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const releaseDir = path.join(root, "src-tauri", "target", "release");
const bundleDir = path.join(releaseDir, "bundle");
const outputDir = path.join(bundleDir, "release-assets");

const assets = [
  {
    label: "Windows setup",
    targetName: "Crux.Addon.Manager.Setup.exe",
    find: () => findFirst(path.join(bundleDir, "nsis"), /^Crux Addon Manager_.*-setup\.exe$/i),
  },
  {
    label: "Windows portable",
    targetName: "Crux.Addon.Manager.Portable.exe",
    find: () => findFirst(releaseDir, /^(eso-addon-manager-desktop|Crux\.Addon\.Manager)\.exe$/i),
  },
  {
    label: "macOS DMG",
    targetName: "Crux.Addon.Manager.macOS.dmg",
    find: () => findFirst(path.join(bundleDir, "dmg"), /^Crux Addon Manager_.*\.dmg$/i),
    optional: true,
  },
];

await mkdir(outputDir, { recursive: true });

let copied = 0;
for (const asset of assets) {
  const source = await asset.find();
  if (!source) {
    const message = `${asset.label} source was not found; run the matching Tauri build target first.`;
    if (asset.optional) {
      console.warn(message);
      continue;
    }
    throw new Error(message);
  }

  const target = path.join(outputDir, asset.targetName);
  await copyFile(source, target);
  copied += 1;
  console.log(`${asset.targetName} <- ${path.relative(root, source)}`);
}

console.log(`Prepared ${copied} release asset${copied === 1 ? "" : "s"} in ${path.relative(root, outputDir)}.`);

async function findFirst(dir, pattern) {
  let entries;
  try {
    entries = await readdir(dir);
  } catch (error) {
    if (error && error.code === "ENOENT") return null;
    throw error;
  }

  const matches = entries.filter((entry) => pattern.test(entry)).sort();
  for (const entry of matches) {
    const candidate = path.join(dir, entry);
    const info = await stat(candidate);
    if (info.isFile()) return candidate;
  }
  return null;
}
