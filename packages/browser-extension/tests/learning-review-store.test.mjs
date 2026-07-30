import assert from "node:assert/strict";
import test from "node:test";

import {
  LearningReviewStore,
} from "../dist/chromium/background/learning-review-store.js";

test("同一会话复用确认身份且拒绝旧标签会话写回", () => {
  const store = new LearningReviewStore();
  const first = store.begin(source("session-a"));
  const replay = store.begin(source("session-a"));
  assert.equal(replay.confirmationID, first.confirmationID);

  assert.equal(
    store.setCandidates(11, "stale-session", [candidate()]),
    null,
  );
  assert.equal(store.pending(11).candidates, null);

  store.discard(11);
  const second = store.begin(source("session-b"));
  assert.notEqual(second.confirmationID, first.confirmationID);
  assert.equal(
    store.setCandidates(11, "session-a", [candidate()]),
    null,
  );
  assert.equal(store.pending(11).candidates, null);
});

test("确认结果只能提交给当前审核的确认身份", () => {
  const store = new LearningReviewStore();
  const pending = store.begin(source("session-a"));
  store.setCandidates(11, "session-a", [candidate()]);

  assert.equal(
    store.complete(11, "stale-confirmation", confirmation()),
    null,
  );
  assert.ok(store.pending(11));
  assert.equal(store.completed(11), null);

  const completed = store.complete(
    11,
    pending.confirmationID,
    confirmation(),
  );
  assert.equal(completed.confirmation.policyCount, 1);
  assert.equal(store.pending(11), null);
});

function source(sessionID) {
  return {
    tabID: 11,
    sessionID,
    normalizedSite: "example.com",
  };
}

function candidate() {
  return {
    normalizedDomain: "example.com",
    kind: "requiredFirstParty",
    confidenceMillis: 1_000,
    requiresConfirmation: false,
    evidenceCount: 1,
    firstSeenAtUnixMilliseconds: 1,
    lastSeenAtUnixMilliseconds: 1,
    mainFrameCount: 1,
    subresourceCount: 0,
    redirectCount: 0,
  };
}

function confirmation() {
  return {
    policies: [
      {
        normalizedDomain: "example.com",
        policyID: "policy-1",
      },
    ],
    snapshotVersion: 7,
    snapshotState: "pendingAck",
    replayed: false,
  };
}
