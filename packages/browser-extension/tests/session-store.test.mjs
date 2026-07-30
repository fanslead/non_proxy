import assert from "node:assert/strict";
import test from "node:test";

import {
  LearningSessionStore,
} from "../dist/chromium/background/session-store.js";

function session(tabID, expiresAtUnixMilliseconds) {
  return {
    tabID,
    sessionID: `session-${tabID}`,
    browserContextID: `context-${tabID}`,
    normalizedSite: `site-${tabID}.example`,
    expiresAtUnixMilliseconds,
  };
}

test("按真实标签页隔离仅存内存的学习状态", () => {
  const store = new LearningSessionStore();
  store.start(session(11, 2_000));
  store.start(session(22, 3_000));
  store.addCandidate(11, "api.example");

  assert.equal(store.size, 2);
  assert.deepEqual(
    [...store.get(11, 1_000).candidateDomains],
    ["api.example"],
  );
  assert.deepEqual([...store.get(22, 1_000).candidateDomains], []);
  assert.equal(store.remove(11).sessionID, "session-11");
  assert.equal(store.get(11, 1_000), null);
  assert.equal(store.size, 1);
});

test("过期读取不返回会话，统一清理可通知上层", () => {
  const store = new LearningSessionStore();
  store.start(session(11, 2_000));
  store.start(session(22, 3_000));

  assert.equal(store.get(11, 2_000), null);
  assert.equal(store.size, 2);
  assert.deepEqual(
    store.removeExpired(2_500).map((value) => value.tabID),
    [11],
  );
  assert.equal(store.size, 1);
});
