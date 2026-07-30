import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = resolve(root, "dist");
const compiled = resolve(dist, "_compiled");
const targets = ["chromium", "safari"];

for (const target of targets) {
  const output = resolve(dist, target);
  await rm(output, { recursive: true, force: true });
  await mkdir(output, { recursive: true });
  await cp(compiled, output, { recursive: true });
  await cp(
    resolve(root, "targets", target, "manifest.json"),
    resolve(output, "manifest.json"),
  );
  await cp(
    resolve(root, "src", "popup", "popup.html"),
    resolve(output, "popup", "popup.html"),
  );
  await cp(
    resolve(root, "src", "popup", "popup.css"),
    resolve(output, "popup", "popup.css"),
  );
  await cp(
    resolve(root, "src", "popup", "candidate-review.css"),
    resolve(output, "popup", "candidate-review.css"),
  );
}

await rm(compiled, { recursive: true, force: true });

const chromiumManifest = JSON.parse(
  await readFile(resolve(dist, "chromium", "manifest.json"), "utf8"),
);
if (chromiumManifest.manifest_version !== 3) {
  throw new Error("Chromium 扩展清单必须使用 Manifest V3。");
}
await writeFile(
  resolve(dist, "BUILD_INFO.json"),
  `${JSON.stringify({ targets, version: chromiumManifest.version }, null, 2)}\n`,
);
