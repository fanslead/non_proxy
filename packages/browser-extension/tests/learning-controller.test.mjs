import assert from "node:assert/strict";
import test from "node:test";

import {
  LearningController,
} from "../dist/chromium/background/learning-controller.js";

class FakeNativeMessenger {
  requests = [];
  closeCount = 0;
  sessionSequence = 0;

  async request(request) {
    this.requests.push(structuredClone(request));
    switch (request.type) {
      case "startLearning":
        this.sessionSequence += 1;
        return {
          sessionID: `session-${this.sessionSequence}`,
          expiresAtUnixMilliseconds: Date.now() + 60_000,
        };
      case "observeLearning":
        return {
          candidate: candidate(request.payload.normalizedDomain),
          duplicate: false,
        };
      case "listCandidates":
        return {
          session: sessionResult(request.payload.sessionID),
          candidates: [],
        };
      case "stopLearning":
        return {
          session: {
            ...sessionResult(request.payload.sessionID),
            state: "stopped",
          },
          candidateCount: 2,
        };
      default:
        throw new Error("unexpected request");
    }
  }

  close() {
    this.closeCount += 1;
  }
}

function candidate(normalizedDomain) {
  return {
    normalizedDomain,
    kind: "unknown",
    confidenceMillis: 500,
    requiresConfirmation: true,
    evidenceCount: 1,
    firstSeenAtUnixMilliseconds: 1,
    lastSeenAtUnixMilliseconds: 1,
    mainFrameCount: 0,
    subresourceCount: 1,
    redirectCount: 0,
  };
}

function sessionResult(sessionID) {
  return {
    sessionID,
    state: "active",
    startedAtUnixMilliseconds: 1,
    expiresAtUnixMilliseconds: Date.now() + 60_000,
  };
}

function request(tabId, url, type = "xmlhttprequest") {
  return {
    tabId,
    requestId: `browser-request-${tabId}`,
    url,
    type,
  };
}

test("多标签页隔离，且只把规范域名发给本地服务", async () => {
  const native = new FakeNativeMessenger();
  const counts = [];
  const controller = new LearningController(
    native,
    undefined,
    (count) => counts.push(count),
  );

  await controller.start(11, "example.com");
  await controller.start(22, "other.example");
  const starts = native.requests.filter(
    (value) => value.type === "startLearning",
  );
  assert.notEqual(
    starts[0].payload.browserContextID,
    starts[1].payload.browserContextID,
  );

  const beforeUnrelatedTab = native.requests.length;
  await controller.observe(
    request(33, "https://ignored.example/private?token=secret"),
    false,
  );
  assert.equal(native.requests.length, beforeUnrelatedTab);

  await controller.observe(
    {
      ...request(
        11,
        "https://API.Example.com/private/path?token=secret",
      ),
      initiator: "https://app.example.com/account?id=private",
    },
    false,
  );
  const observation = native.requests.at(-1);
  assert.equal(observation.type, "observeLearning");
  assert.equal(observation.payload.normalizedDomain, "api.example.com");
  assert.equal(observation.payload.initiatorDomain, "app.example.com");
  assert.equal(JSON.stringify(observation).includes("private"), false);
  assert.equal(JSON.stringify(observation).includes("token"), false);

  for (const nativeRequest of native.requests) {
    assert.equal(hasKey(nativeRequest, "tabID"), false);
    assert.equal(hasKey(nativeRequest, "tabId"), false);
    assert.equal(hasKey(nativeRequest, "url"), false);
  }
  assert.deepEqual(counts, [1, 2]);
});

test("跨站直接导航只结束对应标签页", async () => {
  const native = new FakeNativeMessenger();
  const controller = new LearningController(native);
  await controller.start(11, "example.com");
  await controller.start(22, "other.example");

  await controller.observe(
    request(11, "https://unrelated.example/home", "main_frame"),
    false,
  );

  assert.equal(controller.state(11).active, false);
  assert.equal(controller.state(22).active, true);
  assert.equal(native.closeCount, 0);
  assert.equal(
    native.requests.at(-1).type,
    "stopLearning",
  );

  await controller.stop(22);
  assert.equal(native.closeCount, 1);
});

test("同一浏览器请求的跨域重定向链不会被误判为主动离站", async () => {
  const native = new FakeNativeMessenger();
  const controller = new LearningController(native);
  await controller.start(11, "example.com");

  await controller.observe(
    {
      ...request(11, "https://example.com/login", "main_frame"),
      requestId: "redirect-chain-1",
      redirectUrl: "https://login.identity.example/authorize?secret=x",
    },
    true,
  );
  await controller.observe(
    {
      ...request(
        11,
        "https://login.identity.example/authorize?secret=x",
        "main_frame",
      ),
      requestId: "redirect-chain-1",
    },
    false,
  );

  assert.equal(controller.state(11).active, true);
  const observations = native.requests.filter(
    (value) => value.type === "observeLearning",
  );
  assert.equal(observations.at(-2).payload.kind, "redirect");
  assert.equal(
    observations.at(-2).payload.normalizedDomain,
    "login.identity.example",
  );
  assert.equal(observations.at(-1).payload.kind, "mainFrame");

  await controller.observe(
    {
      ...request(11, "https://unrelated.example/home", "main_frame"),
      requestId: "new-navigation",
    },
    false,
  );
  assert.equal(controller.state(11).active, false);
});

test("权威截止时间会清理状态并关闭 Native 端口", async () => {
  const native = new FakeNativeMessenger();
  const counts = [];
  const controller = new LearningController(
    native,
    undefined,
    (count) => counts.push(count),
  );
  await controller.start(11, "example.com");

  controller.expire(Date.now() + 120_000);

  assert.equal(controller.sessionCount, 0);
  assert.equal(controller.state(11).active, false);
  assert.deepEqual(counts, [1, 0]);
  assert.equal(native.closeCount, 1);
});

function hasKey(value, key) {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  if (Object.prototype.hasOwnProperty.call(value, key)) {
    return true;
  }
  return Object.values(value).some((nested) => hasKey(nested, key));
}
