const q = document.getElementById("q");
const list = document.getElementById("locations");
const near = document.getElementById("near");
const status = document.getElementById("geo-status");
const toNumber = (value) => {
  const number = Number.parseFloat(value || "");
  return Number.isFinite(number) ? number : null;
};
const rows = Array.from(list?.querySelectorAll("li[data-search]") || []).map((node, index) => ({
  node,
  index,
  search: node.dataset.search || "",
  latitude: toNumber(node.dataset.latitude),
  longitude: toNumber(node.dataset.longitude),
  distance: toNumber(node.dataset.distance),
  open: Number.parseInt(node.dataset.open || "1", 10),
  wait: Number.parseInt(node.dataset.wait || "2147483647", 10),
  name: node.dataset.name || "",
  distanceNode: node.querySelector(".distance"),
  hidden: false,
}));
let ordered = rows;
let lastTerm = "";

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

const applyFilter = () => {
  const term = (q?.value || "").trim().toLowerCase();
  if (term === lastTerm) return;
  lastTerm = term;
  for (const row of ordered) {
    const hidden = Boolean(term && !row.search.includes(term));
    if (row.hidden !== hidden) {
      row.hidden = hidden;
      row.node.hidden = hidden;
    }
  }
};

const compareDistance = (a, b) => {
  const distance = (a.distance ?? Infinity) - (b.distance ?? Infinity);
  return distance || a.open - b.open || a.wait - b.wait || a.name.localeCompare(b.name) || a.index - b.index;
};

const reorder = () => {
  if (!list) return;
  const fragment = document.createDocumentFragment();
  for (const row of ordered) fragment.append(row.node);
  list.append(fragment);
  applyFilter();
};

const accuracyWords = (meters) => {
  if (!Number.isFinite(meters)) return "";
  if (meters >= 1609.344) return `, accuracy +/- ${(meters / 1609.344).toFixed(1)} mi`;
  return `, accuracy +/- ${Math.round(meters)} m`;
};

const useBrowserLocation = (position) => {
  const here = position.coords;
  for (const row of rows) {
    if (row.latitude == null || row.longitude == null) {
      row.distance = null;
      continue;
    }
    row.distance = miles(here.latitude, here.longitude, row.latitude, row.longitude);
    if (row.distanceNode) {
      row.distanceNode.textContent = `${row.distance.toFixed(1)} mi`;
      row.distanceNode.hidden = false;
    }
  }
  ordered = rows.slice().sort(compareDistance);
  reorder();
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
