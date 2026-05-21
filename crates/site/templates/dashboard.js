const $ = (id) => document.getElementById(id);
const app = $("app");
const statusEl = $("status");
const graph = $("graph");
const trend = $("trend");
const comparison = $("comparison");
const managed = app?.dataset.managed === "true";
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

let open = app?.dataset.open === "true";
let wait = Number(app?.dataset.wait || 0);
let observedAt = app?.dataset.observedAt || "";
let points = parseSeries(trend?.dataset.v || "");
const comparisonPoints = parseSeries(comparison?.dataset.v || "");
let day = localDay(observedAt || Date.now());

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
  return `${value} ${value === 1 ? "minute" : "minutes"}`;
}

function secondsAgo(value) {
  const seconds = Math.max(0, Math.floor((Date.now() - new Date(value).getTime()) / 1000));
  return `${seconds} ${seconds === 1 ? "second" : "seconds"} ago`;
}

function time(value) {
  const parts = Object.fromEntries(
    timeFmt.formatToParts(new Date(value)).map((part) => [part.type, part.value]),
  );
  return `${parts.hour}:${parts.minute}:${parts.second}${parts.dayPeriod.toLowerCase()}`;
}

function sentence() {
  if (!managed) return statusEl?.textContent || "managed tracking is not enabled yet.";
  if (open && observedAt) {
    return `the wait time at heytea ${locationName} is ${minutes(wait)} as of ${time(observedAt)}, which was ${secondsAgo(observedAt)}`;
  }
  return open ? `waiting for the first managed wait observation at heytea ${locationName}.` : "closed";
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
  if (!managed) return;
  open = payload.isOpen === true;
  wait = payload.pickupWaitMinutes ?? 0;
  observedAt = payload.observedAt || observedAt;
  renderSentence();
  if (graph) graph.hidden = !open || points.length === 0;
  if (!open || payload.pickupWaitMinutes == null || !payload.observedAt) return;

  const nextDay = localDay(payload.observedAt);
  if (nextDay !== day) {
    day = nextDay;
    points = [];
  }
  const minute = minuteOfDay(payload.observedAt);
  const existing = points.find((point) => point.minute === minute);
  if (existing) {
    existing.value = payload.pickupWaitMinutes;
  } else {
    points.push({ minute, value: payload.pickupWaitMinutes });
    points.sort((a, b) => a.minute - b.minute);
  }
  if (graph) graph.hidden = false;
  draw();
}

function connect() {
  const url = app?.dataset.streamUrl;
  if (!managed || !url || !("EventSource" in window)) return;
  const source = new EventSource(url);
  source.addEventListener("status.updated", (event) => {
    try {
      update(JSON.parse(event.data));
    } catch (_) {
      source.close();
    }
  });
}

addEventListener("load", () => {
  draw();
  renderSentence();
  connect();
  setInterval(renderSentence, 1000);
});
