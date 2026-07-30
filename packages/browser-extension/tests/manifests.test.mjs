import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
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

test("Chromium 与 Safari 分发相同的隐私处理代码", async () => {
  for (const relative of [
    "background/learning-controller.js",
    "background/native-port-client.js",
    "shared/domain.js",
    "shared/native-contract.js",
    "popup/popup.js",
  ]) {
    const [chromium, safari] = await Promise.all([
      readFile(new URL(`dist/chromium/${relative}`, root), "utf8"),
      readFile(new URL(`dist/safari/${relative}`, root), "utf8"),
    ]);
    assert.equal(chromium, safari);
  }
});
