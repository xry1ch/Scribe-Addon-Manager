import { invoke } from "@tauri-apps/api/core";
import "./styles.css";
import type {
  AddonDetails,
  AddonSummary,
  CheckAddonsResponse,
  InstalledAddonsResponse,
  PlanUpdatesResponse,
  SearchResponse,
} from "./types";

type Tab = "installed" | "search" | "details" | "updates";

interface AppState {
  tab: Tab;
  path: string;
  loading: boolean;
  error: string | null;
  installed: InstalledAddonsResponse | null;
  searchQuery: string;
  searchLimit: number;
  searchResults: AddonSummary[];
  selectedDetails: AddonDetails | null;
  updates: CheckAddonsResponse | null;
  updatePlan: PlanUpdatesResponse | null;
  includeUnknown: boolean;
}

const state: AppState = {
  tab: "installed",
  path: "",
  loading: false,
  error: null,
  installed: null,
  searchQuery: "",
  searchLimit: 25,
  searchResults: [],
  selectedDetails: null,
  updates: null,
  updatePlan: null,
  includeUnknown: false,
};

const appRoot = document.querySelector<HTMLDivElement>("#app");

if (!appRoot) {
  throw new Error("missing app root");
}

const app = appRoot;

function render() {
  app.innerHTML = `
    <main class="shell">
      <aside class="sidebar">
        <div class="brand">
          <span class="mark"></span>
          <div>
            <h1>Scribe</h1>
            <p>ESO Addon Manager</p>
          </div>
        </div>
        <nav>
          ${tabButton("installed", "Installed")}
          ${tabButton("search", "Search")}
          ${tabButton("details", "Details")}
          ${tabButton("updates", "Updates")}
        </nav>
      </aside>
      <section class="content">
        ${state.error ? `<div class="notice error">${escapeHtml(state.error)}</div>` : ""}
        ${state.loading ? `<div class="notice">Loading...</div>` : ""}
        ${renderCurrentTab()}
      </section>
    </main>
  `;

  bindCommonEvents();
  bindTabEvents();
}

function tabButton(tab: Tab, label: string) {
  return `<button class="nav-button ${state.tab === tab ? "active" : ""}" data-tab="${tab}">${label}</button>`;
}

function renderCurrentTab() {
  if (state.tab === "installed") return renderInstalled();
  if (state.tab === "search") return renderSearch();
  if (state.tab === "details") return renderDetails();
  return renderUpdates();
}

function renderInstalled() {
  const addons = state.installed?.addons ?? [];
  return `
    <header class="toolbar">
      <div>
        <h2>Installed Addons</h2>
        <p>${escapeHtml(state.installed?.addons_dir ?? "No AddOns directory loaded")}</p>
      </div>
      <button class="primary" id="refresh-installed">Refresh</button>
    </header>
    <div class="path-row">
      <label for="path-input">AddOns path</label>
      <input id="path-input" value="${escapeAttr(state.path)}" placeholder="Auto-detect" />
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Folder</th>
            <th>Title</th>
            <th>Version</th>
            <th>API</th>
            <th>Dependencies</th>
            <th>Valid</th>
          </tr>
        </thead>
        <tbody>
          ${addons
            .map(
              (addon) => `
                <tr>
                  <td>${escapeHtml(addon.folder_name)}</td>
                  <td>${escapeHtml(addon.title ?? "-")}</td>
                  <td>${escapeHtml(addon.display_version ?? "-")}</td>
                  <td>${escapeHtml(joinOrDash(addon.api_versions))}</td>
                  <td>${escapeHtml(joinOrDash(addon.depends_on))}</td>
                  <td><span class="pill ${addon.valid_manifest ? "ok" : "bad"}">${addon.valid_manifest ? "yes" : "no"}</span></td>
                </tr>
              `,
            )
            .join("")}
        </tbody>
      </table>
    </div>
  `;
}

function renderSearch() {
  return `
    <header class="toolbar">
      <div>
        <h2>Search</h2>
        <p>Search ESOUI/MMOUI metadata on demand.</p>
      </div>
      <button class="primary" id="run-search">Search</button>
    </header>
    <div class="search-row">
      <input id="search-query" value="${escapeAttr(state.searchQuery)}" placeholder="Addon name, author, or keyword" />
      <select id="search-limit">
        ${[10, 25, 50, 100].map((value) => `<option value="${value}" ${state.searchLimit === value ? "selected" : ""}>${value}</option>`).join("")}
      </select>
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Name</th>
            <th>Author</th>
            <th>Version</th>
            <th>Updated</th>
            <th>UID</th>
          </tr>
        </thead>
        <tbody>
          ${state.searchResults
            .map(
              (addon) => `
                <tr class="click-row" data-addon-id="${escapeAttr(addon.uid ?? "")}">
                  <td>${escapeHtml(addon.name ?? "-")}</td>
                  <td>${escapeHtml(addon.author_name ?? "-")}</td>
                  <td>${escapeHtml(addon.version ?? "-")}</td>
                  <td>${escapeHtml(addon.updated_display ?? "-")}</td>
                  <td>${escapeHtml(addon.uid ?? "-")}</td>
                </tr>
              `,
            )
            .join("")}
        </tbody>
      </table>
    </div>
  `;
}

function renderDetails() {
  const details = state.selectedDetails;
  if (!details) {
    return `
      <header class="toolbar">
        <div>
          <h2>Details</h2>
          <p>Select a search result to inspect addon metadata.</p>
        </div>
      </header>
    `;
  }

  return `
    <header class="toolbar">
      <div>
        <h2>${escapeHtml(details.name ?? "Addon Details")}</h2>
        <p>${escapeHtml(details.uid ?? "-")}</p>
      </div>
    </header>
    <section class="details-grid">
      ${detailItem("UID", details.uid)}
      ${detailItem("Author", details.author_name)}
      ${detailItem("Version", details.version)}
      ${detailItem("Updated", details.updated_display)}
      ${detailItem("Filename", details.file_name)}
      ${detailItem("MD5", details.md5)}
      ${detailItem("Info URL", details.file_info_url)}
      ${detailItem("Download URL", details.download_url)}
    </section>
    ${textBlock("Description", details.description)}
    ${textBlock("Changelog", details.changelog)}
  `;
}

function renderUpdates() {
  const matches = state.updates?.matches ?? [];
  const actions = state.updatePlan?.actions ?? [];
  return `
    <header class="toolbar">
      <div>
        <h2>Updates</h2>
        <p>${escapeHtml(state.updates?.addons_dir ?? "No update check loaded")}</p>
      </div>
      <button class="primary" id="refresh-updates">Refresh</button>
    </header>
    <label class="checkbox-line">
      <input type="checkbox" id="include-unknown" ${state.includeUnknown ? "checked" : ""} />
      Include unknown version matches in the read-only plan
    </label>
    <div class="summary">
      ${summaryItem("Updates", state.updatePlan?.summary.would_update ?? 0)}
      ${summaryItem("Current", state.updatePlan?.summary.current_skipped ?? 0)}
      ${summaryItem("Unknown", state.updatePlan?.summary.unknown ?? 0)}
      ${summaryItem("No match", state.updatePlan?.summary.no_match ?? 0)}
    </div>
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Local folder</th>
            <th>Local version</th>
            <th>Remote name</th>
            <th>Remote version</th>
            <th>Status</th>
            <th>Action</th>
          </tr>
        </thead>
        <tbody>
          ${matches
            .map((match) => {
              const action = actions.find((item) => item.local_folder === match.local.folder_name);
              return `
                <tr>
                  <td>${escapeHtml(match.local.folder_name)}</td>
                  <td>${escapeHtml(match.local.display_version ?? "-")}</td>
                  <td>${escapeHtml(match.remote?.name ?? "-")}</td>
                  <td>${escapeHtml(match.remote?.version ?? "-")}</td>
                  <td><span class="pill">${escapeHtml(match.status)}</span></td>
                  <td>${escapeHtml(action?.action ?? "-")}</td>
                </tr>
              `;
            })
            .join("")}
        </tbody>
      </table>
    </div>
  `;
}

function bindCommonEvents() {
  document.querySelectorAll<HTMLButtonElement>("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.tab = button.dataset.tab as Tab;
      state.error = null;
      render();
    });
  });
}

function bindTabEvents() {
  document.querySelector<HTMLButtonElement>("#refresh-installed")?.addEventListener("click", loadInstalled);
  document.querySelector<HTMLInputElement>("#path-input")?.addEventListener("input", (event) => {
    state.path = (event.currentTarget as HTMLInputElement).value;
  });
  document.querySelector<HTMLButtonElement>("#run-search")?.addEventListener("click", runSearch);
  document.querySelector<HTMLInputElement>("#search-query")?.addEventListener("input", (event) => {
    state.searchQuery = (event.currentTarget as HTMLInputElement).value;
  });
  document.querySelector<HTMLSelectElement>("#search-limit")?.addEventListener("change", (event) => {
    state.searchLimit = Number((event.currentTarget as HTMLSelectElement).value);
  });
  document.querySelectorAll<HTMLTableRowElement>("[data-addon-id]").forEach((row) => {
    row.addEventListener("click", () => loadDetails(row.dataset.addonId ?? ""));
  });
  document.querySelector<HTMLButtonElement>("#refresh-updates")?.addEventListener("click", loadUpdates);
  document.querySelector<HTMLInputElement>("#include-unknown")?.addEventListener("change", (event) => {
    state.includeUnknown = (event.currentTarget as HTMLInputElement).checked;
    loadUpdates();
  });
}

async function withLoading(task: () => Promise<void>) {
  state.loading = true;
  state.error = null;
  render();
  try {
    await task();
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.loading = false;
    render();
  }
}

function loadInstalled() {
  return withLoading(async () => {
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", {
      path: state.path || null,
    });
    state.path = state.installed.addons_dir;
  });
}

function runSearch() {
  return withLoading(async () => {
    const response = await invoke<SearchResponse>("search_remote_addons", {
      query: state.searchQuery,
      limit: state.searchLimit,
    });
    state.searchResults = response.results;
  });
}

function loadDetails(addonId: string) {
  if (!addonId) return;
  state.tab = "details";
  return withLoading(async () => {
    state.selectedDetails = await invoke<AddonDetails>("get_remote_addon_details", {
      addonId,
    });
  });
}

function loadUpdates() {
  return withLoading(async () => {
    state.updates = await invoke<CheckAddonsResponse>("check_addons", {
      path: state.path || null,
    });
    state.updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
      path: state.path || null,
      includeUnknown: state.includeUnknown,
    });
    state.path = state.updates.addons_dir;
  });
}

function detailItem(label: string, value: string | null) {
  return `
    <div class="detail-item">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value ?? "-")}</strong>
    </div>
  `;
}

function textBlock(label: string, value: string | null) {
  if (!value) return "";
  return `
    <section class="text-block">
      <h3>${escapeHtml(label)}</h3>
      <p>${escapeHtml(value)}</p>
    </section>
  `;
}

function summaryItem(label: string, value: number) {
  return `
    <div class="summary-item">
      <span>${escapeHtml(label)}</span>
      <strong>${value}</strong>
    </div>
  `;
}

function joinOrDash(values: string[]) {
  return values.length > 0 ? values.join(", ") : "-";
}

function escapeHtml(value: string) {
  return value.replace(/[&<>"']/g, (char) => {
    const entities: Record<string, string> = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    };
    return entities[char];
  });
}

function escapeAttr(value: string) {
  return escapeHtml(value);
}

render();
loadInstalled();
