import assert from "node:assert/strict";
import test from "node:test";

import {
  validateCandidateList,
} from "../dist/chromium/background/learning-controller-support.js";

test("候选列表拒绝错会话、重复和非规范域名", () => {
  const invalidResults = [
    result([candidate("example.com")], "other-session"),
    result([candidate("example.com"), candidate("example.com")]),
    result([candidate("example.com"), candidate("EXAMPLE.com")]),
    result([candidate("example.com"), candidate("127.0.0.1")]),
    result([
      candidate("example.com"),
      { ...candidate("api.example.com"), kind: "unsafe" },
    ]),
    result([
      candidate("example.com"),
      { ...candidate("api.example.com"), confidenceMillis: 1_001 },
    ]),
  ];

  for (const invalid of invalidResults) {
    assert.throws(
      () => validateCandidateList(pending(), invalid),
      /NP_EXTENSION_NATIVE_RESPONSE_INVALID/,
    );
  }
});

test("候选列表必须包含主站", () => {
  assert.throws(
    () =>
      validateCandidateList(
        pending(),
        result([candidate("api.example.com")]),
      ),
    /NP_EXTENSION_MAIN_CANDIDATE_MISSING/,
  );
});

function pending() {
  return {
    tabID: 11,
    sessionID: "session-a",
    confirmationID: "confirmation-a",
    normalizedSite: "example.com",
    candidates: null,
  };
}

function result(candidates, sessionID = "session-a") {
  return {
    session: {
      sessionID,
      state: "stopped",
      startedAtUnixMilliseconds: 1,
      expiresAtUnixMilliseconds: 2,
    },
    candidates,
  };
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
