import assert from "node:assert/strict";
import test from "node:test";

import {
  normalizeDomain,
  normalizeWebURL,
  staysWithinSite,
} from "../dist/chromium/shared/domain.js";

test("只从 HTTP(S) URL 提取规范域名", () => {
  assert.equal(
    normalizeWebURL("https://BÜCHER.example./private?q=secret#part"),
    "xn--bcher-kva.example",
  );
  assert.equal(normalizeWebURL("http://API.Example.COM:8080/a"), "api.example.com");
  assert.equal(normalizeWebURL("file:///tmp/private"), null);
  assert.equal(normalizeWebURL("chrome://settings"), null);
  assert.equal(normalizeWebURL("https://127.0.0.1/private"), null);
  assert.equal(normalizeWebURL("https://[::1]/private"), null);
});

test("拒绝非规范主机名并限制站点边界", () => {
  assert.equal(normalizeDomain("-api.example.com"), null);
  assert.equal(normalizeDomain("api_.example.com"), null);
  assert.equal(normalizeDomain("example.com."), "example.com");
  assert.equal(staysWithinSite("example.com", "api.example.com"), true);
  assert.equal(staysWithinSite("app.example.com", "example.com"), true);
  assert.equal(staysWithinSite("example.com", "example.net"), false);
  assert.equal(staysWithinSite("example.com", "badexample.com"), false);
});
