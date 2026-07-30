import assert from "node:assert/strict";
import test from "node:test";

import {
  createNativeRequest,
  parseNativeResponse,
} from "../dist/chromium/shared/native-contract.js";

test("Native Messaging 响应必须匹配版本和请求身份", () => {
  const request = createNativeRequest("listCandidates", {
    sessionID: "session-1",
  });
  assert.equal(request.protocolVersion, 1);
  assert.match(request.requestID, /^req-/);
  assert.deepEqual(
    parseNativeResponse(
      {
        protocolVersion: 1,
        requestID: request.requestID,
        ok: true,
        payload: { candidates: [] },
      },
      request.requestID,
    ),
    { candidates: [] },
  );
  assert.throws(
    () =>
      parseNativeResponse(
        {
          protocolVersion: 1,
          requestID: "other",
          ok: true,
          payload: {},
        },
        request.requestID,
      ),
    /NP_EXTENSION_NATIVE_RESPONSE_INVALID/,
  );
});

test("Native Messaging 服务错误保留稳定错误码", () => {
  assert.throws(
    () =>
      parseNativeResponse(
        {
          protocolVersion: 1,
          requestID: "req-1",
          ok: false,
          error: {
            code: "NP_LEARNING_SESSION_EXPIRED",
            message: "会话已过期",
          },
        },
        "req-1",
      ),
    /NP_LEARNING_SESSION_EXPIRED/,
  );
});
