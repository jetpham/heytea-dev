import "./dashboard.css";

const $ = (id: string) => document.getElementById(id);
const app = $("app");
const trend = $("trend") as SVGPolylineElement | null;
let points = (trend?.dataset.v || "").split(",").filter(Boolean).map(Number);
let minuteSlot = Number(trend?.dataset.m || 0);
let open = !app?.className;
let day = new Date().toLocaleDateString("en-US", { timeZone: "America/Los_Angeles" });

const minute = (value: number | null | undefined) => value == null ? "-" : `${value} min`;
const count = (value: number | null | undefined) => value == null ? "-" : String(value);
const time = (value: string | number | null | undefined) => value ? new Date(value).toLocaleTimeString([], { hour: "numeric", minute: "2-digit", second: "2-digit" }) : "after first poll";
const localDay = (value: string | null | undefined) => new Date(value || Date.now()).toLocaleDateString("en-US", { timeZone: "America/Los_Angeles" });
const maxAxis = (value: number) => value <= 5 ? 5 : value <= 10 ? 10 : value <= 15 ? 15 : value <= 20 ? 20 : value <= 30 ? 30 : value <= 45 ? 45 : value <= 60 ? 60 : Math.ceil(value / 30) * 30;

function setText(id: string, value: string) {
  const element = $(id);
  if (element) element.textContent = value;
}

function draw() {
  if (!trend) return;
  const max = maxAxis(Math.max(0, ...points));
  let value = "";
  for (let index = 0; index < points.length; index += 1) {
    value += `${index ? " " : ""}${(points.length < 2 ? 100 : index * 100 / (points.length - 1)).toFixed(1)},${(38 - points[index] / max * 30).toFixed(1)}`;
  }
  trend.setAttribute("points", value);
  setText("ym", `${max}m`);
  setText("xr", time(Date.now()));
}

function update(status: any) {
  setText("w", minute(status.pickupWaitMinutes));
  setText("d", minute(status.deliveryEstimateMinutes));
  setText("c", count(status.makingCups));
  setText("o", count(status.makingOrders));
  setText("obs", time(status.observedAt));
  setText("open", status.isOpen == null ? "unknown" : status.isOpen ? "yes" : "no");
  setText("fresh", status.stale ? "stale" : "fresh");
  setText("notice", status.notice || "No active notice");
  setText("closing", status.closingNotice || "No active closing notice");
  const closed = status.isOpen === false;
  if (app) app.className = closed ? "closed" : "";
  if (closed) {
    points = [];
    open = false;
    draw();
    return;
  }
  if (localDay(status.observedAt) !== day) {
    day = localDay(status.observedAt);
    points = [];
    minuteSlot = 0;
  }
  if (!open) {
    points = [];
    open = true;
  }
  if (status.pickupWaitMinutes != null) {
    const nextMinuteSlot = Math.floor(new Date(status.observedAt).getTime() / 60000);
    if (nextMinuteSlot !== minuteSlot) {
      points.push(status.pickupWaitMinutes);
      minuteSlot = nextMinuteSlot;
    } else if (points.length) {
      points[points.length - 1] = status.pickupWaitMinutes;
    }
    if (points.length > 1440) points.shift();
    draw();
  }
}

try {
  const streamUrl = app?.dataset.streamUrl;
  if (streamUrl) {
    const events = new EventSource(streamUrl);
    events.onopen = () => setText("s", "live");
    events.onerror = () => setText("s", "retrying");
    events.addEventListener("status.updated", (event) => update(JSON.parse((event as MessageEvent).data)));
  }
} catch {
}
