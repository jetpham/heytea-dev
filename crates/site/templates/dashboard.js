const $ = (id) => document.getElementById(id);
const app = $("app");
const statusEl = $("status");
const graph = $("graph");
const trend = $("trend");
const comparison = $("comparison");
const streamUrl = app?.dataset.streamUrl || "";
const zone = app?.dataset.timezone || "America/Los_Angeles";
const locationName = (app?.dataset.locationName || "downtown metreon").toLowerCase();
const timeFmt = new Intl.DateTimeFormat("en-US", {
  timeZone: zone,
  hour: "numeric",
  minute: "2-digit",
  second: "2-digit",
  hour12: true,
});
const minuteFmt = new Intl.DateTimeFormat("en-US", {
  timeZone: zone,
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
});

let open = parseOpen(app?.dataset.open || "");
let wait = parseOptionalNumber(app?.dataset.wait || "");
let observedAt = app?.dataset.observedAt || "";
let points = parseSeries(trend?.dataset.v || "");
let comparisonPoints = parseSeries(comparison?.dataset.v || "");
let day = localDay(observedAt || Date.now());
let source = null;
let pollTimer = 0;

function parseOpen(raw) {
  if (raw === "true") return true;
  if (raw === "false") return false;
  return null;
}

function parseOptionalNumber(raw) {
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

function parseSeries(raw) {
  return raw
    .split(",")
    .filter(Boolean)
    .map((item) => {
      const [minute, value] = item.split(":").map(Number);
      return { minute, value };
    })
    .filter((point) => Number.isFinite(point.minute) && Number.isFinite(point.value));
}

function minutes(value) {
  const rounded = Math.round(value);
  return `${rounded} ${rounded === 1 ? "minute" : "minutes"}`;
}

function secondsAgo(value) {
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 1000));
  return `${seconds} ${seconds === 1 ? "second" : "seconds"} ago`;
}

function time(value) {
  const parts = Object.fromEntries(
    timeFmt.formatToParts(new Date(value)).map((part) => [part.type, part.value]),
  );
  return `${parts.hour}:${parts.minute}:${parts.second}${(parts.dayPeriod || "").toLowerCase()}`;
}

function sentence() {
  if (observedAt) {
    if (open === false) return "closed";
    if (wait != null) {
      return `the wait time at heytea ${locationName} is ${minutes(wait)} as of ${time(observedAt)}, which was ${secondsAgo(observedAt)}`;
    }
    return `live wait time at heytea ${locationName} is unavailable as of ${time(observedAt)}, which was ${secondsAgo(observedAt)}`;
  }
  return `waiting for the first tracked wait observation at heytea ${locationName}.`;
}

function renderSentence() {
  if (statusEl) statusEl.textContent = sentence();
}

function localDay(value) {
  return new Date(value || Date.now()).toLocaleDateString("en-US", { timeZone: zone });
}

function minuteOfDay(value) {
  const parts = Object.fromEntries(
    minuteFmt.formatToParts(new Date(value)).map((part) => [part.type, part.value]),
  );
  return (Number(parts.hour) % 24) * 60 + Number(parts.minute);
}

function maxAxis(value) {
  if (value <= 5) return 5;
  if (value <= 10) return 10;
  if (value <= 15) return 15;
  if (value <= 20) return 20;
  if (value <= 30) return 30;
  if (value <= 45) return 45;
  if (value <= 60) return 60;
  return Math.ceil(value / 30) * 30;
}

function drawSeries(el, series, axis) {
  if (!el) return;
  const path = series
    .map((point) => {
      const x = Math.max(0, Math.min(1439, point.minute)) * 100 / 1439;
      const y = 38 - point.value / axis * 30;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  el.setAttribute("points", path);
}

function draw() {
  const max = Math.max(0, ...points.map((point) => point.value), ...comparisonPoints.map((point) => point.value));
  const axis = maxAxis(max);
  drawSeries(comparison, comparisonPoints, axis);
  drawSeries(trend, points, axis);
}

function update(payload) {
  open = parseOpen(String(payload.isOpen));
  wait = payload.pickupWaitMinutes == null ? null : parseOptionalNumber(payload.pickupWaitMinutes);
  observedAt = payload.observedAt || observedAt;
  renderSentence();
  if (graph) graph.hidden = open !== true || points.length === 0;
  if (open !== true || wait == null || !payload.observedAt) return;

  const nextDay = localDay(payload.observedAt);
  if (nextDay !== day) {
    day = nextDay;
    points = [];
  }
  const minute = minuteOfDay(payload.observedAt);
  const existing = points.find((point) => point.minute === minute);
  if (existing) {
    existing.value = wait;
  } else {
    points.push({ minute, value: wait });
    points.sort((a, b) => a.minute - b.minute);
  }
  if (graph) graph.hidden = false;
  draw();
}

function connect() {
  if (source || !streamUrl) return;
  if (!("EventSource" in window)) {
    startPolling();
    return;
  }
  source = new EventSource(streamUrl);
  source.addEventListener("status.updated", (event) => {
    try {
      update(JSON.parse(event.data));
    } catch (_) {
      disconnect();
    }
  });
}

function disconnect() {
  if (!source) return;
  source.close();
  source = null;
}

function statusUrl() {
  if (streamUrl.endsWith("/stream")) {
    return `${streamUrl.slice(0, -"/stream".length)}/status`;
  }
  return "";
}

function historyUrl() {
  if (streamUrl.endsWith("/stream")) {
    return `${streamUrl.slice(0, -"/stream".length)}/history?range=today`;
  }
  return "";
}

function historyPoints(items) {
  if (!Array.isArray(items)) return [];
  return items
    .map((point) => ({
      minute: minuteOfDay(point.start),
      value: Math.round(Number(point.avgPickupWaitMinutes)),
    }))
    .filter((point) => Number.isFinite(point.minute) && Number.isFinite(point.value));
}

function comparisonHistoryPoints(items) {
  if (!Array.isArray(items)) return [];
  return items
    .map((point) => ({
      minute: Number(point.minuteOfDay),
      value: Math.round(Number(point.avgPickupWaitMinutes)),
    }))
    .filter((point) => Number.isFinite(point.minute) && Number.isFinite(point.value));
}

async function loadHistory() {
  const url = historyUrl();
  if (!url) return;
  const response = await fetch(url);
  if (!response.ok) return;
  const payload = await response.json();
  points = historyPoints(payload.points);
  comparisonPoints = comparisonHistoryPoints(payload.comparisonPoints);
  if (graph) graph.hidden = open !== true || points.length === 0;
  draw();
}

async function pollOnce() {
  const url = statusUrl();
  if (!url) return;
  try {
    const response = await fetch(url, { cache: "no-store" });
    if (response.ok) {
      update(await response.json());
    } else {
      renderSentence();
    }
  } catch (_) {
    renderSentence();
  }
  startPolling(60000);
}

function startPolling(delay = 0) {
  if (pollTimer) return;
  pollTimer = setTimeout(() => {
    pollTimer = 0;
    pollOnce();
  }, delay);
}

addEventListener("load", () => {
  draw();
  renderSentence();
  loadHistory().catch(() => {});
  connect();
  setInterval(renderSentence, 1000);
});
