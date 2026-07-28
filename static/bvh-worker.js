import {SnapshotReader} from "./render-data-snapshot.js";
import {DerivedBvh} from "./bvh-core.js";

let reader, bvh = new DerivedBvh(), epoch = 0, updating = false, requestedEpoch = 0;
function ensureEpoch(expected) {
  if (!reader) return false;
  const latest = reader.latest();
  if (latest.epoch !== expected) return false;
  if (epoch === expected) return true;
  const result = reader.transaction(snapshot => { bvh.update(snapshot); epoch = snapshot.epoch; }, expected);
  return result !== null && epoch === expected;
}
function coalescedUpdate(hint = 0) {
  requestedEpoch = Math.max(requestedEpoch, hint >>> 0);
  if (updating) return;
  updating = true;
  queueMicrotask(() => { try { const latest = reader?.latest(); if (latest?.epoch && latest.epoch !== epoch) ensureEpoch(latest.epoch); if (epoch) postMessage({type: "updated", epoch}); } catch (error) { postMessage({type: "fatal", code: "PICK_PROTOCOL_MISMATCH", message: String(error)}); } finally { updating = false; if (requestedEpoch > epoch) coalescedUpdate(); } });
}
addEventListener("message", event => {
  const m = event.data;
  try {
    if (m.type === "init") { reader = new SnapshotReader(m.memory, m.controlPtr); if (m.controlVersion !== 1 || m.schemaVersion !== 2) throw Object.assign(new Error("version"), {code: "PICK_PROTOCOL_MISMATCH"}); postMessage({type: "ready"}); }
    else if (m.type === "update") coalescedUpdate(m.epoch);
    else if (m.type === "pick") { if (!ensureEpoch(m.epoch)) { postMessage({type: "pick", request: m.request, stale: true, epoch}); return; } const hits = bvh.pick(m.origin, m.direction, m.maxDistance, m.maxHits); const latest = reader.latest().epoch; postMessage({type: "pick", request: m.request, stale: latest !== m.epoch || epoch !== m.epoch, epoch, hits}); }
    else if (m.type === "dispose") close();
  } catch (error) { postMessage({type: "fatal", code: error.name === "SnapshotProtocolError" || error.code === "PICK_PROTOCOL_MISMATCH" ? "PICK_PROTOCOL_MISMATCH" : "PICK_WORKER_ERROR", message: String(error)}); }
});
