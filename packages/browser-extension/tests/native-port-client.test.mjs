import assert from "node:assert/strict";
import test from "node:test";

import {
  NativePortClient,
} from "../dist/chromium/background/native-port-client.js";
import {
  createNativeRequest,
} from "../dist/chromium/shared/native-contract.js";

class FakeEvent {
  listeners = [];

  addListener(listener) {
    this.listeners.push(listener);
  }

  emit(value) {
    for (const listener of this.listeners) {
      listener(value);
    }
  }
}

class FakePort {
  onMessage = new FakeEvent();
  onDisconnect = new FakeEvent();
  requests = [];

  constructor(action) {
    this.action = action;
  }

  postMessage(request) {
    this.requests.push(structuredClone(request));
    queueMicrotask(() => this.action(this, request));
  }

  disconnect() {
    this.onDisconnect.emit();
  }
}

function browserWithPorts(actions) {
  const ports = [];
  return {
    ports,
    api: {
      runtime: {
        onMessage: new FakeEvent(),
        connectNative() {
          const action = actions[ports.length];
          assert.ok(action);
          const port = new FakePort(action);
          ports.push(port);
          return port;
        },
        async sendMessage() {},
      },
    },
  };
}

test("传输断开后用同一请求身份重试一次", async () => {
  const fake = browserWithPorts([
    (port) => port.onDisconnect.emit(),
    (port, request) =>
      port.onMessage.emit({
        protocolVersion: 1,
        requestID: request.requestID,
        ok: true,
        payload: { accepted: true },
      }),
  ]);
  const client = new NativePortClient(fake.api);
  const request = createNativeRequest("listCandidates", {
    sessionID: "session-1",
  });

  assert.deepEqual(await client.request(request), { accepted: true });
  assert.equal(fake.ports.length, 2);
  assert.equal(
    fake.ports[0].requests[0].requestID,
    fake.ports[1].requests[0].requestID,
  );
});

test("服务端业务错误不会重放请求", async () => {
  const fake = browserWithPorts([
    (port, request) =>
      port.onMessage.emit({
        protocolVersion: 1,
        requestID: request.requestID,
        ok: false,
        error: {
          code: "NP_LEARNING_SESSION_EXPIRED",
          message: "expired",
        },
      }),
  ]);
  const client = new NativePortClient(fake.api);
  const request = createNativeRequest("listCandidates", {
    sessionID: "session-1",
  });

  await assert.rejects(
    () => client.request(request),
    /NP_LEARNING_SESSION_EXPIRED/,
  );
  assert.equal(fake.ports.length, 1);
});
