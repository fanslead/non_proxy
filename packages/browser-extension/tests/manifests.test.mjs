import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function json(relative) {
  return JSON.parse(await readFile(new URL(relative, root), "utf8"));
}

test("浏览器清单仅申请学习所需最小权限", async () => {
  for (const target of ["chromium", "safari"]) {
    const manifest = await json(`dist/${target}/manifest.json`);
    assert.equal(manifest.manifest_version, 3);
    assert.deepEqual(
      [...manifest.permissions].sort(),
      ["activeTab", "nativeMessaging", "webRequest"].sort(),
    );
    assert.deepEqual(manifest.optional_host_permissions, ["*://*/*"]);
    assert.equal("host_permissions" in manifest, false);
    assert.equal("content_scripts" in manifest, false);
  }
});

test("Chromium 公钥稳定生成 Native Host 允许的固定扩展 ID", async () => {
  const manifest = await json("dist/chromium/manifest.json");
  const digest = createHash("sha256")
    .update(Buffer.from(manifest.key, "base64"))
    .digest()
    .subarray(0, 16);
  const extensionID = [...digest]
    .flatMap((byte) => [byte >> 4, byte & 0x0f])
    .map((nibble) => String.fromCharCode("a".charCodeAt(0) + nibble))
    .join("");
  assert.equal(extensionID, "ldiadofihjimpkhchjicmgcfgjlgidha");
});

test("Chromium 与 Safari 分发相同的共享隐私处理代码", async () => {
  const [chromiumFiles, safariFiles] = await Promise.all([
    distributionFiles("dist/chromium"),
    distributionFiles("dist/safari"),
  ]);
  assert.deepEqual(chromiumFiles, safariFiles);
  for (const relative of chromiumFiles) {
    if (relative === "background/background.js") {
      continue;
    }
    const [chromium, safari] = await Promise.all([
      readFile(new URL(`dist/chromium/${relative}`, root)),
      readFile(new URL(`dist/safari/${relative}`, root)),
    ]);
    assert.deepEqual(chromium, safari);
  }
});

test("Safari 后台入口是不依赖模块加载的单文件产物", async () => {
  const manifest = await json("dist/safari/manifest.json");
  assert.deepEqual(manifest.background, {
    scripts: ["background/background.js"],
  });
  const background = await readFile(
    new URL("dist/safari/background/background.js", root),
    "utf8",
  );
  assert.equal(/^\s*(?:import|export)\s/m.test(background), false);
  assert.match(background, /connectNative/);
  assert.match(background, /confirmLearning/);
  assert.equal(manifest.icons["512"], "icons/nonproxy.svg");
  assert.equal(
    manifest.action.default_icon["32"],
    "icons/nonproxy.svg",
  );
});

async function distributionFiles(relativeRoot, relative = "") {
  const entries = await readdir(
    new URL(`${relativeRoot}/${relative}`, root),
    { withFileTypes: true },
  );
  const files = [];
  for (const entry of entries) {
    const path = relative ? `${relative}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      files.push(...(await distributionFiles(relativeRoot, path)));
    } else if (path !== "manifest.json") {
      files.push(path);
    }
  }
  return files.sort();
}
