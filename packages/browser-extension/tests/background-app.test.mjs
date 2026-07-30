import assert from "node:assert/strict";
import test from "node:test";

import {
  BackgroundApp,
} from "../dist/chromium/background/background-app.js";

function fakeBrowser({ permission = true, tabURL = "https://example.com" } = {}) {
  const removed = [];
  return {
    removed,
    api: {
      permissions: {
        async contains() {
          return permission;
        },
        async remove(value) {
          removed.push(value);
          return true;
        },
      },
      tabs: {
        async get(tabID) {
          return { id: tabID, url: tabURL };
        },
      },
    },
  };
}

test("后台状态丢失时立即回收临时全站权限", async () => {
  const browser = fakeBrowser();
  const learning = {
    sessionCount: 0,
    async refresh() {
      return { active: false, candidateCount: 0 };
    },
  };
  const app = new BackgroundApp(browser.api, learning);

  assert.deepEqual(
    await app.handleMessage({ type: "getState", tabID: 11 }),
    {
      ok: true,
      state: { active: false, candidateCount: 0 },
    },
  );
  assert.deepEqual(browser.removed, [{ origins: ["*://*/*"] }]);
});

test("未授权或不支持的页面不能启动学习", async () => {
  const denied = fakeBrowser({ permission: false });
  const learning = {
    sessionCount: 0,
    async start() {
      throw new Error("should not start");
    },
  };
  const deniedApp = new BackgroundApp(denied.api, learning);
  assert.deepEqual(
    await deniedApp.handleMessage({ type: "start", tabID: 11 }),
    { ok: false, error: "NP_EXTENSION_PERMISSION_REQUIRED" },
  );

  const unsupported = fakeBrowser({ tabURL: "chrome://settings" });
  const unsupportedApp = new BackgroundApp(unsupported.api, learning);
  assert.deepEqual(
    await unsupportedApp.handleMessage({ type: "start", tabID: 11 }),
    { ok: false, error: "NP_EXTENSION_SITE_UNSUPPORTED" },
  );
});
