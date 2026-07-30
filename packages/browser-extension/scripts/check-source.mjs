import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const manifestPaths = [
  "targets/chromium/manifest.json",
  "targets/safari/manifest.json",
];
const forbiddenPermissions = new Set([
  "history",
  "cookies",
  "tabs",
  "webRequestBlocking",
]);

for (const relative of manifestPaths) {
  const manifest = JSON.parse(
    await readFile(resolve(root, relative), "utf8"),
  );
  const permissions = new Set(manifest.permissions ?? []);
  for (const permission of forbiddenPermissions) {
    if (permissions.has(permission)) {
      throw new Error(`${relative} 包含禁止权限 ${permission}。`);
    }
  }
  if (manifest.host_permissions?.length) {
    throw new Error(`${relative} 不得声明常驻 host_permissions。`);
  }
}

const contract = await readFile(
  resolve(root, "src/shared/native-contract.ts"),
  "utf8",
);
for (const forbidden of [
  "fullURL",
  "requestHeaders",
  "responseHeaders",
  "pageContent",
]) {
  if (contract.includes(forbidden)) {
    throw new Error(`Native Messaging 契约包含禁止字段 ${forbidden}。`);
  }
}
