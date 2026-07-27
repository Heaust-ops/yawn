import assert from "node:assert/strict";
import test from "node:test";
import { DirtyReason, RenderScheduler, requestViewInvalidations } from "@lib/worker/render-scheduler.js";

test("atlas cleanup invalidations request worker viewport repaints", () => {
  const requests: Array<readonly [number, DirtyReason]> = [];
  const views = new Map([
    ["a", { scheduler: { request: (id: number, reason: DirtyReason) => requests.push([id, reason]) } }],
  ]);
  requestViewInvalidations(["missing", "a"], views);
  assert.deepEqual(requests, [[0, DirtyReason.Viewport]]);
});

test("continuous RAF polls while idle and preserves one-frame backpressure", () => {
  const callbacks: Array<() => void> = [];
  const draws: Array<readonly [number, number, number]> = [];
  let polls = 0;
  const scheduler = new RenderScheduler(
    (...args) => draws.push(args),
    (callback) => callbacks.push(callback),
  );
  const tick = () => {
    const callback = callbacks.shift();
    assert.ok(callback);
    callback();
  };

  scheduler.start(() => polls++);
  assert.equal(callbacks.length, 1);
  tick();
  assert.equal(polls, 1);
  assert.equal(draws.length, 0);
  assert.equal(callbacks.length, 1);

  scheduler.request(4, DirtyReason.Preview);
  tick();
  assert.deepEqual(draws, [[1, 4, DirtyReason.Preview]]);
  scheduler.request(5, DirtyReason.Scene);
  tick();
  assert.equal(draws.length, 1);
  scheduler.consumed(99);
  tick();
  assert.equal(draws.length, 1);
  scheduler.consumed(1);
  tick();
  assert.deepEqual(draws[1], [2, 5, DirtyReason.Scene]);
  assert.equal(scheduler.metrics.staleAcks, 1);
  assert.equal(scheduler.metrics.maxInFlight, 1);

  scheduler.stop();
  tick();
  assert.equal(callbacks.length, 0);
});

test("requests coalesce and only a matching defer retries the same reasons", () => {
  const callbacks: Array<() => void> = [],
    draws: Array<readonly [number, number, number]> = [];
  const scheduler = new RenderScheduler(
    (...args) => draws.push(args),
    (callback) => callbacks.push(callback),
  );
  const tick = () => callbacks.shift()!();
  scheduler.start();
  scheduler.request(2, DirtyReason.Scene);
  scheduler.request(7, DirtyReason.Selection);
  tick();
  assert.deepEqual(draws, [[1, 7, DirtyReason.Scene | DirtyReason.Selection]]);

  scheduler.defer(99, DirtyReason.Camera);
  tick();
  assert.equal(draws.length, 1, "a stale defer must not clear the in-flight frame");
  scheduler.defer(1, DirtyReason.Scene | DirtyReason.Selection);
  tick();
  assert.deepEqual(draws[1], [2, 7, DirtyReason.Scene | DirtyReason.Selection]);

  scheduler.consumed(1);
  tick();
  assert.equal(draws.length, 2, "a stale ack must not consume the retried frame");
  scheduler.consumed(2);
  tick();
  assert.equal(draws.length, 2);
  scheduler.stop();
});
