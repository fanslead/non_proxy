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

test("后台只转发经过校验的候选选择并支持丢弃审核", async () => {
  const browser = fakeBrowser();
  const calls = [];
  const learning = {
    sessionCount: 0,
    async confirm(tabID, selectedDomains) {
      calls.push(["confirm", tabID, [...selectedDomains]]);
      return {
        active: false,
        candidateCount: selectedDomains.length,
        confirmation: {
          policyCount: selectedDomains.length,
          snapshotVersion: 7,
          snapshotState: "pendingAck",
          replayed: false,
        },
      };
    },
    discard(tabID) {
      calls.push(["discard", tabID]);
      return { active: false, candidateCount: 0 };
    },
  };
  const app = new BackgroundApp(browser.api, learning);

  const confirmed = await app.handleMessage({
    type: "confirm",
    tabID: 11,
    selectedDomains: ["example.com", "api.example.com"],
    sessionID: "must-not-cross-boundary",
    confirmationID: "must-not-cross-boundary",
  });
  assert.equal(confirmed.ok, true);
  assert.deepEqual(calls[0], [
    "confirm",
    11,
    ["example.com", "api.example.com"],
  ]);

  assert.deepEqual(
    await app.handleMessage({ type: "discard", tabID: 11 }),
    {
      ok: true,
      state: { active: false, candidateCount: 0 },
    },
  );
  assert.deepEqual(calls[1], ["discard", 11]);
});

test("后台拒绝空选择、重复域名和超出上限的确认消息", async () => {
  const browser = fakeBrowser();
  const learning = {
    sessionCount: 0,
    async confirm() {
      throw new Error("must not confirm");
    },
  };
  const app = new BackgroundApp(browser.api, learning);
  const invalid = [
    [],
    ["example.com", "example.com"],
    Array.from({ length: 257 }, (_, index) => `${index}.example.com`),
    ["x".repeat(254)],
  ];

  for (const selectedDomains of invalid) {
    assert.deepEqual(
      await app.handleMessage({
        type: "confirm",
        tabID: 11,
        selectedDomains,
      }),
      { ok: false, error: "NP_EXTENSION_REQUEST_INVALID" },
    );
  }
});
