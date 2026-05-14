import { createEffect, createSignal, onCleanup, onMount } from "solid-js";
import { render } from "solid-js/web";
import type { ECharts } from "./chart";
import "./styles.css";

const API_URL = import.meta.env.VITE_API_URL ?? "https://api.heytea.dev";

type Status = {
  name: string;
  address: string;
  isOpen: boolean | null;
  pickupWaitMinutes: number | null;
  deliveryEstimateMinutes: number | null;
  makingCups: number | null;
  makingOrders: number | null;
  notice: string | null;
  closingNotice: string | null;
  observedAt: string;
  stale: boolean;
  staleAfter: string;
};

type History = {
  range: string;
  bucket: string;
  points: Array<{
    start: string;
    avgPickupWaitMinutes: number | null;
    maxPickupWaitMinutes: number | null;
  }>;
};

function App() {
  const [status, setStatus] = createSignal<Status | null>(null);
  const [history, setHistory] = createSignal<History | null>(null);
  const [streamState, setStreamState] = createSignal("connecting");
  const [chart, setChart] = createSignal<ECharts | null>(null);
  let chartEl: HTMLDivElement | undefined;
  let chartObserver: IntersectionObserver | undefined;
  let chartLoading = false;

  async function refresh() {
    const [statusRes, historyRes] = await Promise.all([
      fetch(`${API_URL}/status`),
      fetch(`${API_URL}/history?range=24h&bucket=5m`),
    ]);
    if (statusRes.ok) setStatus(await statusRes.json());
    if (historyRes.ok) setHistory(await historyRes.json());
  }

  onMount(() => {
    void refresh();
    const loadChart = async () => {
      if (!chartEl || chartLoading || chart()) return;
      chartLoading = true;
      const instance = await createChart(chartEl);
      setChart(instance);
    };

    if (chartEl && "IntersectionObserver" in window) {
      chartObserver = new IntersectionObserver((entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          chartObserver?.disconnect();
          void loadChart();
        }
      });
      chartObserver.observe(chartEl);
    } else {
      void loadChart();
    }

    const events = new EventSource(`${API_URL}/stream`);
    events.addEventListener("open", () => setStreamState("live"));
    events.addEventListener("error", () => setStreamState("reconnecting"));
    events.addEventListener("status.updated", (event) => {
      setStatus(JSON.parse((event as MessageEvent).data));
    });
    onCleanup(() => {
      events.close();
      chartObserver?.disconnect();
      chart()?.dispose();
    });
  });

  createEffect(() => {
    const instance = chart();
    const data = history();
    if (!instance || !data) return;
    instance.setOption({
      grid: { left: 36, right: 16, top: 20, bottom: 32 },
      xAxis: {
        type: "category",
        data: data.points.map((point) => new Date(point.start).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })),
        axisLabel: { color: "#79836f" },
      },
      yAxis: { type: "value", axisLabel: { color: "#79836f", formatter: "{value}m" } },
      series: [
        {
          name: "Average pickup wait",
          type: "line",
          smooth: true,
          symbol: "none",
          lineStyle: { width: 3, color: "#7ddf64" },
          areaStyle: { color: "rgba(125, 223, 100, 0.14)" },
          data: data.points.map((point) => point.avgPickupWaitMinutes),
        },
      ],
    });
  });

  const current = () => status();

  return (
    <main class="shell">
      <section class="hero">
        <div>
          <p class="eyebrow">heytea.dev</p>
          <h1>Downtown Metreon wait time</h1>
          <p class="subtle">Live status for HeyTea at 165 4th St, San Francisco.</p>
        </div>
        <div class={`stream ${streamState()}`}>{streamState()}</div>
      </section>

      <section class="status-grid">
        <article class="card wait-card">
          <span class="label">Pickup wait</span>
          <strong>{formatMinutes(current()?.pickupWaitMinutes)}</strong>
          <span class="subtle">Observed {formatObserved(current()?.observedAt)}</span>
        </article>
        <article class="card">
          <span class="label">Delivery estimate</span>
          <strong>{formatMinutes(current()?.deliveryEstimateMinutes)}</strong>
        </article>
        <article class="card">
          <span class="label">Cups in progress</span>
          <strong>{current()?.makingCups ?? "—"}</strong>
        </article>
        <article class="card">
          <span class="label">Orders in progress</span>
          <strong>{current()?.makingOrders ?? "—"}</strong>
        </article>
      </section>

      <section class="details">
        <article class="panel">
          <h2>Store state</h2>
          <dl>
            <div><dt>Open</dt><dd>{formatOpen(current()?.isOpen)}</dd></div>
            <div><dt>Freshness</dt><dd>{current()?.stale ? "stale" : "fresh"}</dd></div>
            <div><dt>Notice</dt><dd>{current()?.notice ?? "No active notice"}</dd></div>
            <div><dt>Closing notice</dt><dd>{current()?.closingNotice ?? "No active closing notice"}</dd></div>
          </dl>
        </article>
        <article class="panel chart-panel">
          <h2>Last 24 hours</h2>
          <div ref={chartEl} class="chart" />
        </article>
      </section>
    </main>
  );
}

async function createChart(el: HTMLDivElement): Promise<ECharts> {
  const { createWaitChart } = await import("./chart");
  return createWaitChart(el);
}

function formatMinutes(value: number | null | undefined) {
  return value == null ? "—" : `${value} min`;
}

function formatObserved(value: string | undefined) {
  if (!value) return "after first poll";
  return new Date(value).toLocaleTimeString([], { hour: "numeric", minute: "2-digit", second: "2-digit" });
}

function formatOpen(value: boolean | null | undefined) {
  if (value == null) return "unknown";
  return value ? "yes" : "no";
}

render(() => <App />, document.getElementById("root")!);
