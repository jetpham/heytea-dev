const q = document.getElementById("q");
const list = document.getElementById("locations");
const near = document.getElementById("near");
const status = document.getElementById("geo-status");
const streamUrl = list?.dataset.streamUrl || "";
const data = (() => {
  try {
    const value = JSON.parse(document.getElementById("locations-data")?.textContent || "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
})();
const toNumber = (value) => {
  if (value == null || value === "") return null;
  const number = typeof value === "number" ? value : Number.parseFloat(value);
  return Number.isFinite(number) ? number : null;
};
const [latitudes = [], longitudes = [], addresses = [], openSorts = [], waitSorts = []] = data;
const text = (value) => value == null ? "" : String(value);
const slugFromHref = (href) => href?.startsWith("/") ? href.slice(1) : href || "";
const createNode = (row) => {
  const node = document.createElement("li");
  const link = document.createElement("a");
  const meta = document.createElement("span");
  const wait = document.createElement("span");
  const distance = document.createElement("span");
  link.setAttribute("href", `/${row.slug}`);
  link.textContent = row.name;
  meta.className = "meta";
  wait.className = "wait";
  wait.textContent = row.statusText;
  distance.className = "distance";
  distance.textContent = row.distanceText;
  distance.hidden = row.distanceHidden;
  meta.append(wait, distance);
  node.append(link, meta);
  row.node = node;
  row.distanceNode = distance;
  return node;
};
const locationNodes = Array.from(list?.querySelectorAll("li") || []).filter((node) => node.querySelector("a[href]"));
const rows = locationNodes.map((node, index) => {
  const link = node.querySelector("a[href]");
  const distanceNode = node.querySelector(".distance");
  const slug = slugFromHref(link?.getAttribute("href"));
  const name = link?.textContent || "";
  const wait = toNumber(waitSorts[index]);
  return {
    node,
    index,
    slug,
    name,
    search: `${name} ${slug} ${addresses[index] || ""}`.toLowerCase(),
    latitude: toNumber(latitudes[index]),
    longitude: toNumber(longitudes[index]),
    distanceNode,
    distance: toNumber(distanceNode?.textContent),
    distanceText: distanceNode?.textContent || "",
    distanceHidden: !distanceNode || distanceNode.hidden || !distanceNode.textContent,
    open: toNumber(openSorts[index]) ?? 1,
    wait: wait ?? 2147483647,
    statusText: node.querySelector(".wait")?.textContent || "",
  };
});
let ordered = rows;
let visibleRows = rows;
let lastTerm = (q?.value || "").trim().toLowerCase();
let browserLocation = null;

const miles = (fromLatitude, fromLongitude, toLatitude, toLongitude) => {
  const radius = 3958.8;
  const dLatitude = (toLatitude - fromLatitude) * Math.PI / 180;
  const dLongitude = (toLongitude - fromLongitude) * Math.PI / 180;
  const aLatitude = fromLatitude * Math.PI / 180;
  const bLatitude = toLatitude * Math.PI / 180;
  const h = Math.sin(dLatitude / 2) ** 2 + Math.cos(aLatitude) * Math.cos(bLatitude) * Math.sin(dLongitude / 2) ** 2;
  const clamped = Math.min(1, Math.max(0, h));
  return 2 * radius * Math.atan2(Math.sqrt(clamped), Math.sqrt(1 - clamped));
};

const updateDistance = (row, text, hidden) => {
  if (row.distanceText === text && row.distanceHidden === hidden) return;
  row.distanceText = text;
  row.distanceHidden = hidden;
  if (row.distanceNode) {
    row.distanceNode.textContent = text;
    row.distanceNode.hidden = hidden;
  }
};

const updateBrowserDistance = (row, here) => {
  if (row.latitude == null || row.longitude == null) {
    row.distance = null;
    updateDistance(row, "", true);
    return;
  }
  row.distance = miles(here.latitude, here.longitude, row.latitude, row.longitude);
  updateDistance(row, `${row.distance.toFixed(1)} mi`, false);
};

const lisPositions = (values) => {
  const predecessors = new Array(values.length).fill(-1);
  const tails = [];
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (value < 0) continue;
    let low = 0;
    let high = tails.length;
    while (low < high) {
      const mid = (low + high) >> 1;
      if (values[tails[mid]] < value) low = mid + 1;
      else high = mid;
    }
    if (low > 0) predecessors[index] = tails[low - 1];
    tails[low] = index;
  }
  const positions = new Set();
  for (let index = tails[tails.length - 1]; index != null && index >= 0; index = predecessors[index]) {
    positions.add(index);
  }
  return positions;
};

const replaceRows = (nextRows) => {
  list.replaceChildren(...nextRows.map((row) => row.node));
};

const reorderRows = (nextRows) => {
  const previousIndexes = new Map(visibleRows.map((row, index) => [row, index]));
  const nextIndexes = nextRows.map((row) => previousIndexes.get(row) ?? -1);
  const keep = lisPositions(nextIndexes);
  if (nextRows.length - keep.size > nextRows.length / 4) {
    replaceRows(nextRows);
    return;
  }
  let anchor = null;
  for (let index = nextRows.length - 1; index >= 0; index -= 1) {
    const row = nextRows[index];
    if (keep.has(index)) {
      anchor = row.node;
    } else {
      list.insertBefore(row.node, anchor);
      anchor = row.node;
    }
  }
};

const render = (term = (q?.value || "").trim().toLowerCase(), minimal = false) => {
  if (!list) return;
  const visible = term ? ordered.filter((row) => row.search.includes(term)) : ordered;
  if (minimal && visible.length === visibleRows.length && visible.length === rows.length) {
    reorderRows(visible);
  } else {
    replaceRows(visible);
  }
  visibleRows = visible;
  lastTerm = term;
};

const applyFilter = () => {
  const term = (q?.value || "").trim().toLowerCase();
  if (term === lastTerm) return;
  render(term);
};

const appendRows = (nextRows) => {
  if (nextRows.length === 0) return;
  rows.push(...nextRows);
  ordered = browserLocation ? rows.slice().sort(compareDistance) : rows;
  if (!browserLocation && !lastTerm) {
    list.append(...nextRows.map((row) => row.node));
    visibleRows = visibleRows.concat(nextRows);
  } else {
    render(lastTerm, Boolean(browserLocation));
  }
};

const rowFromStream = (parts) => {
  if (!Array.isArray(parts)) return null;
  const index = toNumber(parts[0]);
  const slug = text(parts[1]);
  const name = text(parts[2]);
  if (!slug || !name) return null;
  const wait = toNumber(parts[7]);
  const row = {
    node: null,
    index: index ?? rows.length,
    slug,
    name,
    search: `${name} ${slug} ${text(parts[3])}`.toLowerCase(),
    latitude: toNumber(parts[4]),
    longitude: toNumber(parts[5]),
    distanceNode: null,
    distance: toNumber(parts[9]),
    distanceText: text(parts[9]),
    distanceHidden: !parts[9],
    open: toNumber(parts[6]) ?? 1,
    wait: wait ?? 2147483647,
    statusText: text(parts[8]),
  };
  createNode(row);
  if (browserLocation) updateBrowserDistance(row, browserLocation);
  return row;
};

const loadRemaining = async () => {
  if (!streamUrl || !("fetch" in window)) return;
  const response = await fetch(streamUrl, { headers: { Accept: "application/x-ndjson" } });
  if (!response.ok) return;
  const batch = [];
  const flush = () => {
    if (batch.length === 0) return;
    appendRows(batch.splice(0));
  };
  const pushLine = (line) => {
    if (!line) return;
    try {
      const row = rowFromStream(JSON.parse(line));
      if (row) batch.push(row);
      if (batch.length >= 100) flush();
    } catch {
      // Ignore malformed rows; the next refresh will try again.
    }
  };
  if (!response.body) {
    for (const line of (await response.text()).split("\n")) pushLine(line);
    flush();
    return;
  }
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  for (;;) {
    const { value, done } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() || "";
    for (const line of lines) pushLine(line);
  }
  buffer += decoder.decode();
  pushLine(buffer);
  flush();
};

const compareDistance = (a, b) => {
  const distance = (a.distance ?? Infinity) - (b.distance ?? Infinity);
  return distance || a.open - b.open || a.wait - b.wait || a.name.localeCompare(b.name) || a.index - b.index;
};

const accuracyWords = (meters) => {
  if (!Number.isFinite(meters)) return "";
  if (meters >= 1609.344) return `, accuracy +/- ${(meters / 1609.344).toFixed(1)} mi`;
  return `, accuracy +/- ${Math.round(meters)} m`;
};

const useBrowserLocation = (position) => {
  const here = position.coords;
  browserLocation = here;
  for (const row of rows) {
    updateBrowserDistance(row, here);
  }
  ordered = rows.slice().sort(compareDistance);
  render(undefined, true);
  const nearest = ordered.find((row) => row.distance != null);
  if (status && nearest) {
    status.textContent = `browser location active${accuracyWords(here.accuracy)}; nearest store is ${nearest.name} at ${nearest.distance.toFixed(1)} mi.`;
  }
  if (near) {
    near.disabled = false;
    near.textContent = "update my location";
  }
};

q?.addEventListener("input", applyFilter);
q?.addEventListener("search", applyFilter);
near?.addEventListener("click", () => {
  if (!navigator.geolocation) {
    if (status) status.textContent = "browser location is not available in this browser.";
    return;
  }
  near.disabled = true;
  near.textContent = "locating...";
  navigator.geolocation.getCurrentPosition(
    useBrowserLocation,
    () => {
      if (status) status.textContent = "browser location was not allowed; keeping server order.";
      near.disabled = false;
      near.textContent = "use my location";
    },
    { enableHighAccuracy: false, maximumAge: 300000, timeout: 10000 },
  );
});

const prefetched = new Set();
const prefetch = (target) => {
  const linkTarget = target?.closest?.("a[href^='/']");
  const href = linkTarget?.getAttribute("href");
  if (!href || prefetched.has(href)) return;
  prefetched.add(href);
  const link = document.createElement("link");
  link.rel = "prefetch";
  link.as = "document";
  link.href = href;
  document.head.append(link);
};

list?.addEventListener("pointerover", (event) => prefetch(event.target));
list?.addEventListener("focusin", (event) => prefetch(event.target));
list?.addEventListener("touchstart", (event) => prefetch(event.target), { passive: true });
addEventListener("load", () => {
  loadRemaining().catch(() => {});
});
