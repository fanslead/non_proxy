import assert from "node:assert/strict";
import test from "node:test";

import {
  LearningController,
} from "../dist/chromium/background/learning-controller.js";

class FakeNativeMessenger {
  requests = [];
  closeCount = 0;
  sessionSequence = 0;
  failConfirmCount = 0;
  candidates = new Map();
  states = new Map();

  async request(request) {
    this.requests.push(structuredClone(request));
    switch (request.type) {
      case "startLearning": {
        this.sessionSequence += 1;
        const sessionID = `session-${this.sessionSequence}`;
        this.candidates.set(sessionID, new Map());
        this.states.set(sessionID, "active");
        return {
          sessionID,
          expiresAtUnixMilliseconds: Date.now() + 60_000,
        };
      }
      case "observeLearning": {
        const result = candidate(
          request.payload.normalizedDomain,
          request.payload.kind === "mainFrame",
        );
        this.candidates
          .get(request.payload.sessionID)
          .set(result.normalizedDomain, result);
        return { candidate: result, duplicate: false };
      }
      case "listCandidates": {
        const candidates = [
          ...this.candidates.get(request.payload.sessionID).values(),
        ];
        return {
          session: this.#sessionResult(request.payload.sessionID),
          candidates,
        };
      }
      case "stopLearning":
        this.states.set(request.payload.sessionID, "stopped");
        return {
          session: this.#sessionResult(request.payload.sessionID),
          candidateCount:
            this.candidates.get(request.payload.sessionID)?.size ?? 0,
        };
      case "confirmLearning":
        if (this.failConfirmCount > 0) {
          this.failConfirmCount -= 1;
          throw new Error("NP_STORAGE_SNAPSHOT_PENDING_EXISTS");
        }
        return {
          policies: request.payload.selectedDomains.map(
            (normalizedDomain, index) => ({
              normalizedDomain,
              policyID: `policy-${index + 1}`,
            }),
          ),
          snapshotVersion: 7,
          snapshotState: "pendingAck",
          replayed: false,
        };
      default:
        throw new Error("unexpected request");
    }
  }

  close() {
    this.closeCount += 1;
  }

  #sessionResult(sessionID) {
    return {
      sessionID,
      state: this.states.get(sessionID) ?? "active",
      startedAtUnixMilliseconds: 1,
      expiresAtUnixMilliseconds: Date.now() + 60_000,
    };
  }
}

function candidate(normalizedDomain, main = false) {
  return {
    normalizedDomain,
    kind: main ? "requiredFirstParty" : "unknown",
    confidenceMillis: main ? 1_000 : 500,
    requiresConfirmation: !main,
    evidenceCount: 1,
    firstSeenAtUnixMilliseconds: 1,
    lastSeenAtUnixMilliseconds: 1,
    mainFrameCount: main ? 1 : 0,
    subresourceCount: main ? 0 : 1,
    redirectCount: 0,
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
  const starts = requestsOfType(native, "startLearning");
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

test("跨站导航只结束对应标签页并保留独立审核", async () => {
  const native = new FakeNativeMessenger();
  const controller = new LearningController(native);
  await controller.start(11, "example.com");
  await controller.start(22, "other.example");

  await controller.observe(
    request(11, "https://unrelated.example/home", "main_frame"),
    false,
  );

  assert.equal(controller.state(11).active, false);
  assert.equal(
    controller.state(11).review.normalizedSite,
    "example.com",
  );
  assert.equal(controller.state(22).active, true);
  assert.equal(native.closeCount, 0);

  await controller.stop(22);
  assert.deepEqual(
    controller
      .state(11)
      .review.candidates.map((value) => value.normalizedDomain),
    ["example.com"],
  );
  assert.deepEqual(
    controller
      .state(22)
      .review.candidates.map((value) => value.normalizedDomain),
    ["other.example"],
  );
  await controller.discard(11);
  assert.equal(native.closeCount, 0);
  await controller.discard(22);
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
  const observations = requestsOfType(native, "observeLearning");
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
  assert.ok(controller.state(11).review);
});

test("权威截止时间停止会话但保留候选审核和 Native 端口", async () => {
  const native = new FakeNativeMessenger();
  const counts = [];
  const controller = new LearningController(
    native,
    undefined,
    (count) => counts.push(count),
  );
  await controller.start(11, "example.com");

  await controller.expire(Date.now() + 120_000);

  assert.equal(controller.sessionCount, 0);
  assert.equal(controller.state(11).active, false);
  assert.equal(controller.state(11).review.candidates.length, 1);
  assert.deepEqual(counts, [1, 0]);
  assert.equal(native.closeCount, 0);
});

test("确认只接受该标签页候选且强制包含主站", async () => {
  const native = new FakeNativeMessenger();
  const controller = new LearningController(native);
  await controller.start(11, "example.com");
  await controller.observe(
    request(11, "https://api.example.com/private?token=x"),
    false,
  );
  await controller.stop(11);

  await assert.rejects(
    controller.confirm(11, ["api.example.com"]),
    /NP_EXTENSION_SELECTION_INVALID/,
  );
  await assert.rejects(
    controller.confirm(11, ["example.com", "unknown.example"]),
    /NP_EXTENSION_SELECTION_INVALID/,
  );
  assert.equal(requestsOfType(native, "confirmLearning").length, 0);

  const state = await controller.confirm(11, [
    "example.com",
    "api.example.com",
  ]);
  assert.equal(state.confirmation.policyCount, 2);
  assert.equal(state.confirmation.snapshotState, "pendingAck");
  const confirmation = requestsOfType(native, "confirmLearning")[0];
  assert.deepEqual(confirmation.payload.selectedDomains, [
    "example.com",
    "api.example.com",
  ]);
  assert.equal(hasKey(confirmation, "tabID"), false);
  assert.equal(hasKey(confirmation, "tabId"), false);
  assert.equal(hasKey(confirmation, "url"), false);
  assert.equal(native.closeCount, 1);
});

test("确认业务失败保留勾选上下文和稳定幂等身份", async () => {
  const native = new FakeNativeMessenger();
  native.failConfirmCount = 1;
  const controller = new LearningController(native);
  await controller.start(11, "example.com");
  await controller.stop(11);

  await assert.rejects(
    controller.confirm(11, ["example.com"]),
    /NP_STORAGE_SNAPSHOT_PENDING_EXISTS/,
  );
  assert.ok(controller.state(11).review);
  assert.equal(native.closeCount, 0);

  const state = await controller.confirm(11, ["example.com"]);
  assert.equal(state.confirmation.policyCount, 1);
  const confirmations = requestsOfType(native, "confirmLearning");
  assert.equal(confirmations.length, 2);
  assert.equal(
    confirmations[0].payload.confirmationID,
    confirmations[1].payload.confirmationID,
  );
  assert.notEqual(
    confirmations[0].requestID,
    confirmations[1].requestID,
  );
});

test("关闭标签页会丢弃待审核状态且不留下真实标签身份", async () => {
  const native = new FakeNativeMessenger();
  const controller = new LearningController(native);
  await controller.start(11, "example.com");
  await controller.stop(11);
  const requestCount = native.requests.length;

  await controller.release(11);

  assert.deepEqual(controller.state(11), {
    active: false,
    candidateCount: 0,
  });
  assert.equal(native.requests.length, requestCount);
  assert.equal(native.closeCount, 1);
});

test("活动学习会话不能被审核丢弃消息隐藏", async () => {
  const native = new FakeNativeMessenger();
  const controller = new LearningController(native);
  await controller.start(11, "example.com");

  await assert.rejects(
    controller.discard(11),
    /NP_EXTENSION_SESSION_ACTIVE/,
  );

  assert.equal(controller.state(11).active, true);
  assert.equal(controller.sessionCount, 1);
  assert.equal(native.closeCount, 0);
});

function requestsOfType(native, type) {
  return native.requests.filter((value) => value.type === type);
}

function hasKey(value, key) {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  if (Object.prototype.hasOwnProperty.call(value, key)) {
    return true;
  }
  return Object.values(value).some((nested) => hasKey(nested, key));
}
