import { invoke } from "@tauri-apps/api/core";
import "./styles.css";
import type {
  AddonDetails,
  AddonSummary,
  AppSettings,
  AppSettingsInput,
  ApplyUpdateAllResponse,
  CheckAddonsResponse,
  InstallRemoteAddonResponse,
  InstalledAddonsResponse,
  PlanRemoteInstallResponse,
  PlanUpdateAllResponse,
  PlanUpdatesResponse,
  SearchResponse,
  SingleUpdateApplyResponse,
  SingleUpdatePlanResponse,
} from "./types";

type Tab = "installed" | "search" | "details" | "updates" | "settings";

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
  installPlan: PlanRemoteInstallResponse | null;
  installResult: InstallRemoteAddonResponse | null;
  forceUpdate: boolean;
  singleUpdatePlan: SingleUpdatePlanResponse | null;
  singleUpdateResult: SingleUpdateApplyResponse | null;
  updateAllPlan: PlanUpdateAllResponse | null;
  updateAllResult: ApplyUpdateAllResponse | null;
  settings: AppSettings | null;
  addonsPathExists: boolean | null;
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
  installPlan: null,
  installResult: null,
  forceUpdate: false,
  singleUpdatePlan: null,
  singleUpdateResult: null,
  updateAllPlan: null,
  updateAllResult: null,
  settings: null,
  addonsPathExists: null,
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
          ${tabButton("settings", "Settings")}
        </nav>
      </aside>
      <section class="content">
        ${state.error ? `<div class="notice error">${escapeHtml(state.error)}</div>` : ""}
        ${state.loading ? `<div class="notice">Working...</div>` : ""}
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
  if (state.tab === "settings") return renderSettings();
  return renderUpdates();
}

function renderSettings() {
  const settings = state.settings;
  const addonsMissing = Boolean(settings?.addons_dir_override) && state.addonsPathExists === false;
  return `
    <header class="toolbar">
      <div>
        <h2>Settings</h2>
        <p>Desktop defaults for install and update actions.</p>
      </div>
      <div class="toolbar-actions">
        <button class="secondary" id="reset-settings" ${disabledAttr()}>Reset</button>
        <button class="primary" id="save-settings" ${disabledAttr()}>Save</button>
      </div>
    </header>
    ${addonsMissing ? `<div class="notice error">Configured AddOns path does not exist: ${escapeHtml(settings?.addons_dir_override ?? "")}</div>` : ""}
    <div class="notice">Blank paths use auto-detection or built-in defaults. Backup and download directories are created only when an install or update writes files there.</div>
    <section class="details-grid">
      ${settingField("AddOns path override", "settings-addons-dir", settings?.addons_dir_override ?? "")}
      ${settingField("Backup directory override", "settings-backup-dir", settings?.backup_dir_override ?? "")}
      ${settingField("Download directory", "settings-download-dir", settings?.download_dir ?? "")}
      ${settingCheckbox("Keep downloads default", "settings-keep-downloads", settings?.keep_downloads_default ?? false)}
      ${settingCheckbox("Include unknown updates default", "settings-include-unknown", settings?.include_unknown_updates_default ?? false)}
    </section>
  `;
}

function renderInstalled() {
  const addons = state.installed?.addons ?? [];
  return `
    <header class="toolbar">
      <div>
        <h2>Installed Addons</h2>
        ${pathDisplay(state.installed?.addons_dir ?? "No AddOns directory loaded")}
      </div>
      <button class="primary" id="refresh-installed" ${disabledAttr()}>Refresh</button>
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
          ${
            addons.length === 0
              ? emptyRow(6, state.installed ? "No installed addons were found in this AddOns directory." : "Refresh to scan your AddOns directory.")
              : addons
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
                  .join("")
          }
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
        <p>Search remote addon metadata on demand.</p>
      </div>
      <button class="primary" id="run-search" ${disabledAttr()}>Search</button>
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
          ${
            state.searchResults.length === 0
              ? emptyRow(5, state.searchQuery.trim() ? "No remote addons matched this search." : "Enter a search term, then run Search.")
              : state.searchResults
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
                  .join("")
          }
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
      <button class="primary" id="plan-install" ${disabledAttr()}>Plan Install</button>
    </header>
    <div class="path-row">
      <label for="details-path-input">AddOns path</label>
      <input id="details-path-input" value="${escapeAttr(state.path)}" placeholder="Auto-detect" />
    </div>
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
    ${renderInstallPlan()}
    ${renderInstallResult()}
    ${textBlock("Description", details.description)}
    ${textBlock("Changelog", details.changelog)}
  `;
}

function renderInstallPlan() {
  const plan = state.installPlan;
  if (!plan) return "";

  return `
    <section class="plan-panel">
      <div class="notice">Dry run only. This preview downloaded and validated the ZIP, but did not install, update, delete, back up, or extract anything into the real AddOns directory.</div>
      <div class="toolbar compact">
        <div>
          <h3>Install Preview</h3>
          <p>Review this plan before continuing.</p>
        </div>
        <button class="danger" id="confirm-install" ${disabledAttr()}>Install</button>
      </div>
      <section class="details-grid">
        ${detailItem("Remote name", plan.remote.name)}
        ${detailItem("UID", plan.remote.uid)}
        ${detailItem("Version", plan.remote.version)}
        ${detailItem("Filename", plan.remote.file_name)}
        ${detailItem("MD5", plan.remote.md5)}
        ${detailItem("Target AddOns directory", plan.addons_dir)}
      </section>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Source folder</th>
              <th>Title</th>
              <th>Version</th>
              <th>Target folder</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            ${plan.plan.items
              .map(
                (item) => `
                  <tr>
                    <td>${escapeHtml(item.source_folder ?? "-")}</td>
                    <td>${escapeHtml(item.title ?? "-")}</td>
                    <td>${escapeHtml(item.version ?? "-")}</td>
                    <td>${escapeHtml(item.target_folder ?? "-")}</td>
                    <td><span class="pill">${escapeHtml(item.action)}</span></td>
                  </tr>
                `,
              )
              .join("")}
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderInstallResult() {
  const result = state.installResult;
  if (!result) return "";

  return `
    <section class="plan-panel">
      <div class="notice ${result.applied ? "" : "error"}">
        Install ${result.applied ? "completed" : "finished without file changes"}.
      </div>
      <div class="summary">
        ${summaryItem("Installed", result.installed_new)}
        ${summaryItem("Replaced", result.replaced)}
        ${summaryItem("Skipped", result.skipped)}
        ${summaryItem("Applied", result.applied ? 1 : 0)}
      </div>
      <section class="details-grid">
        ${detailItem("Backup location", result.backup_dir)}
        ${detailItem("AddOns directory", result.addons_dir)}
      </section>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Source folder</th>
              <th>Target folder</th>
              <th>Backup folder</th>
              <th>Result</th>
            </tr>
          </thead>
          <tbody>
            ${result.items
              .map(
                (item) => `
                  <tr>
                    <td>${escapeHtml(item.source_folder ?? "-")}</td>
                    <td>${escapeHtml(item.target_folder ?? "-")}</td>
                    <td>${escapeHtml(item.backup_folder ?? "-")}</td>
                    <td><span class="pill">${escapeHtml(item.action)}</span></td>
                  </tr>
                `,
              )
              .join("")}
          </tbody>
        </table>
      </div>
      <button class="primary" id="refresh-installed-after-install" ${disabledAttr()}>Refresh Installed</button>
    </section>
  `;
}

function renderUpdates() {
  const matches = state.updates?.matches ?? [];
  const actions = state.updatePlan?.actions ?? [];
  return `
    <header class="toolbar">
      <div>
        <h2>Updates</h2>
        ${pathDisplay(state.updates?.addons_dir ?? "No update check loaded")}
      </div>
      <div class="toolbar-actions">
        <button class="secondary" id="plan-update-all" ${disabledAttr()} ${state.updatePlan ? "" : "disabled"}>Plan All Updates</button>
        <button class="primary" id="refresh-updates" ${disabledAttr()}>Refresh</button>
      </div>
    </header>
    <label class="checkbox-line">
      <input type="checkbox" id="include-unknown" ${state.includeUnknown ? "checked" : ""} ${disabledAttr()} />
      Include unknown version matches in update plans
    </label>
    <label class="checkbox-line">
      <input type="checkbox" id="force-update" ${state.forceUpdate ? "checked" : ""} ${disabledAttr()} />
      Allow single-addon reinstall planning for current, local-newer, or unknown-version matches
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
            <th>Plan</th>
          </tr>
        </thead>
        <tbody>
          ${
            matches.length === 0
              ? emptyRow(7, state.updates ? "No installed addons were found to check." : "Refresh to check installed addons against remote metadata.")
              : matches
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
                  <td>${renderUpdatePlanButton(match.status, match.local.folder_name)}</td>
                </tr>
              `;
                  })
                  .join("")
          }
        </tbody>
      </table>
    </div>
    ${renderUpdateAllPlan()}
    ${renderUpdateAllResult()}
    ${renderSingleUpdatePlan()}
    ${renderSingleUpdateResult()}
  `;
}

function renderUpdateAllPlan() {
  const plan = state.updateAllPlan;
  if (!plan) return "";

  return `
    <section class="plan-panel">
      <div class="notice">Dry run only. This plan did not download, extract, modify, or delete addon files.</div>
      <div class="toolbar compact">
        <div>
          <h3>All Updates Preview</h3>
          <p>${plan.summary.planned_updates} planned update${plan.summary.planned_updates === 1 ? "" : "s"} in ${escapeHtml(plan.addons_dir)}</p>
        </div>
        ${plan.summary.planned_updates > 0 ? `<button class="danger" id="apply-update-all" ${disabledAttr()}>Apply All Updates</button>` : ""}
      </div>
      <div class="summary">
        ${summaryItem("Planned", plan.summary.planned_updates)}
        ${summaryItem("Current", plan.summary.skipped_current)}
        ${summaryItem("Unknown", plan.summary.skipped_unknown)}
        ${summaryItem("No match", plan.summary.skipped_no_match)}
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Local folder</th>
              <th>Local version</th>
              <th>Remote name</th>
              <th>Remote version</th>
              <th>Plan action</th>
              <th>Update-all</th>
            </tr>
          </thead>
          <tbody>
            ${
              plan.actions.length === 0
                ? emptyRow(6, "No update candidates were found in this plan.")
                : plan.actions
                    .map(
                      (action) => `
                  <tr>
                    <td>${escapeHtml(action.local_folder)}</td>
                    <td>${escapeHtml(action.local_version ?? "-")}</td>
                    <td>${escapeHtml(action.remote_name ?? "-")}</td>
                    <td>${escapeHtml(action.remote_version ?? "-")}</td>
                    <td><span class="pill">${escapeHtml(action.action)}</span></td>
                    <td><span class="pill ${action.update_all_action === "would-update" ? "ok" : ""}">${escapeHtml(action.update_all_action)}</span></td>
                  </tr>
                `,
                    )
                    .join("")
            }
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderUpdateAllResult() {
  const result = state.updateAllResult;
  if (!result) return "";

  return `
    <section class="plan-panel">
      <div class="notice ${result.applied ? "" : "error"}">
        Update-all ${result.applied ? "completed" : "finished without file changes"}.
      </div>
      <div class="summary">
        ${summaryItem("Updated", result.results.length)}
        ${summaryItem("Planned", result.summary.planned_updates)}
        ${summaryItem("Applied", result.applied ? 1 : 0)}
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Local folder</th>
              <th>Remote</th>
              <th>Installed</th>
              <th>Replaced</th>
              <th>Skipped</th>
              <th>Backup</th>
            </tr>
          </thead>
          <tbody>
            ${
              result.results.length === 0
                ? emptyRow(6, "No addons were updated.")
                : result.results
                    .map(
                      (item) => `
                  <tr>
                    <td>${escapeHtml(item.target.local_folder)}</td>
                    <td>${escapeHtml(item.remote_details.name ?? item.target.remote_name ?? "-")}</td>
                    <td>${item.installed_new}</td>
                    <td>${item.replaced}</td>
                    <td>${item.skipped}</td>
                    <td>${escapeHtml(item.backup_dir ?? "-")}</td>
                  </tr>
                `,
                    )
                    .join("")
            }
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderUpdatePlanButton(status: string, target: string) {
  if (status === "possible-update") {
    return `<button class="primary small" data-plan-update-target="${escapeAttr(target)}" ${disabledAttr()}>Plan Update</button>`;
  }

  if (state.forceUpdate && ["matched", "unknown-update", "local-newer"].includes(status)) {
    return `<button class="secondary small" data-plan-update-target="${escapeAttr(target)}" ${disabledAttr()}>Plan Reinstall</button>`;
  }

  return "-";
}

function renderSingleUpdatePlan() {
  const plan = state.singleUpdatePlan;
  if (!plan) return "";

  if (!plan.should_install || !plan.plan) {
    return `
      <section class="plan-panel">
        <div class="notice error">
          ${escapeHtml(plan.local.folder_name)} skipped: ${escapeHtml(plan.reason ?? plan.decision)}
        </div>
      </section>
    `;
  }

  return `
    <section class="plan-panel">
      <div class="notice">Dry run only. This update preview downloaded and validated the ZIP, but did not modify your AddOns directory.</div>
      <div class="toolbar compact">
        <div>
          <h3>Update Preview</h3>
          <p>${escapeHtml(plan.local.folder_name)} -> ${escapeHtml(plan.remote?.name ?? "-")}</p>
        </div>
        <button class="danger" id="confirm-update" ${disabledAttr()}>Update</button>
      </div>
      <section class="details-grid">
        ${detailItem("Decision", plan.decision)}
        ${detailItem("Remote UID", plan.remote?.uid ?? null)}
        ${detailItem("Remote version", plan.remote?.version ?? null)}
        ${detailItem("Target AddOns directory", plan.addons_dir)}
      </section>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Source folder</th>
              <th>Title</th>
              <th>Version</th>
              <th>Target folder</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            ${plan.plan.items
              .map(
                (item) => `
                  <tr>
                    <td>${escapeHtml(item.source_folder ?? "-")}</td>
                    <td>${escapeHtml(item.title ?? "-")}</td>
                    <td>${escapeHtml(item.version ?? "-")}</td>
                    <td>${escapeHtml(item.target_folder ?? "-")}</td>
                    <td><span class="pill">${escapeHtml(item.action)}</span></td>
                  </tr>
                `,
              )
              .join("")}
          </tbody>
        </table>
      </div>
    </section>
  `;
}

function renderSingleUpdateResult() {
  const result = state.singleUpdateResult;
  if (!result) return "";

  return `
    <section class="plan-panel">
      <div class="notice ${result.applied ? "" : "error"}">
        Update ${result.applied ? "completed" : `skipped: ${escapeHtml(result.reason ?? result.decision)}`}.
      </div>
      <div class="summary">
        ${summaryItem("Installed", result.installed_new)}
        ${summaryItem("Replaced", result.replaced)}
        ${summaryItem("Skipped", result.skipped)}
        ${summaryItem("Applied", result.applied ? 1 : 0)}
      </div>
      <section class="details-grid">
        ${detailItem("Backup location", result.backup_dir)}
        ${detailItem("AddOns directory", result.addons_dir)}
      </section>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Source folder</th>
              <th>Target folder</th>
              <th>Backup folder</th>
              <th>Result</th>
            </tr>
          </thead>
          <tbody>
            ${result.items
              .map(
                (item) => `
                  <tr>
                    <td>${escapeHtml(item.source_folder ?? "-")}</td>
                    <td>${escapeHtml(item.target_folder ?? "-")}</td>
                    <td>${escapeHtml(item.backup_folder ?? "-")}</td>
                    <td><span class="pill">${escapeHtml(item.action)}</span></td>
                  </tr>
                `,
              )
              .join("")}
          </tbody>
        </table>
      </div>
    </section>
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
  document.querySelector<HTMLButtonElement>("#plan-install")?.addEventListener("click", planInstall);
  document.querySelector<HTMLButtonElement>("#confirm-install")?.addEventListener("click", confirmInstall);
  document
    .querySelector<HTMLButtonElement>("#refresh-installed-after-install")
    ?.addEventListener("click", loadInstalled);
  document.querySelector<HTMLInputElement>("#details-path-input")?.addEventListener("input", (event) => {
    state.path = (event.currentTarget as HTMLInputElement).value;
  });
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
  document.querySelector<HTMLButtonElement>("#plan-update-all")?.addEventListener("click", planUpdateAll);
  document.querySelector<HTMLButtonElement>("#apply-update-all")?.addEventListener("click", applyUpdateAll);
  document.querySelector<HTMLInputElement>("#include-unknown")?.addEventListener("change", (event) => {
    state.includeUnknown = (event.currentTarget as HTMLInputElement).checked;
    state.updateAllPlan = null;
    state.updateAllResult = null;
    loadUpdates();
  });
  document.querySelector<HTMLInputElement>("#force-update")?.addEventListener("change", (event) => {
    state.forceUpdate = (event.currentTarget as HTMLInputElement).checked;
    state.singleUpdatePlan = null;
    state.singleUpdateResult = null;
    render();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-plan-update-target]").forEach((button) => {
    button.addEventListener("click", () => planSingleUpdate(button.dataset.planUpdateTarget ?? ""));
  });
  document.querySelector<HTMLButtonElement>("#confirm-update")?.addEventListener("click", confirmUpdate);
  document.querySelector<HTMLButtonElement>("#save-settings")?.addEventListener("click", saveSettings);
  document.querySelector<HTMLButtonElement>("#reset-settings")?.addEventListener("click", resetSettings);
  document.querySelector<HTMLInputElement>("#settings-addons-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-backup-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-download-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-keep-downloads")?.addEventListener("change", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-include-unknown")?.addEventListener("change", syncSettingsDraft);
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
    if (!state.settings) {
      state.settings = await invoke<AppSettings>("get_app_settings");
      applySettingsToState(state.settings);
    }
    state.addonsPathExists = await invoke<boolean>("path_exists", {
      path: effectiveAddonsPath(),
    });
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", {
      path: effectiveAddonsPath(),
    });
    state.path = state.installed.addons_dir;
  });
}

function runSearch() {
  if (!state.searchQuery.trim()) {
    state.searchResults = [];
    render();
    return;
  }

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
    state.installPlan = null;
    state.installResult = null;
  });
}

function planInstall() {
  const addonId = state.selectedDetails?.uid;
  if (!addonId) return;

  return withLoading(async () => {
    state.installPlan = await invoke<PlanRemoteInstallResponse>("plan_remote_install", {
      addonId,
      path: effectiveAddonsPath(),
    });
    state.path = state.installPlan.addons_dir;
    state.installResult = null;
  });
}

function confirmInstall() {
  const addonId = state.selectedDetails?.uid;
  const plan = state.installPlan;
  if (!addonId || !plan) return;

  const backupText = plan.plan.items.some((item) => item.action === "would-replace-existing")
    ? "Existing folders may be backed up and replaced. A default timestamped backup folder will be created inside the AddOns directory unless the app is configured otherwise."
    : "No replacement is currently planned, so no backup folder is expected unless the fresh install plan changes.";
  const confirmed = window.confirm(
    `Install ${plan.remote.name ?? addonId}?\n\nFiles may be written to your AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n\nThe app will fetch fresh metadata, download and verify the ZIP, validate it, build a fresh plan, and back up replacements before applying.`,
  );
  if (!confirmed) return;

  return withLoading(async () => {
    state.installResult = await invoke<InstallRemoteAddonResponse>("install_remote_addon", {
      addonId,
      path: effectiveAddonsPath(),
      backupDir: state.settings?.backup_dir_override || null,
      keepDownload: state.settings?.keep_downloads_default ?? false,
      downloadDir: state.settings?.download_dir || null,
    });
    state.path = state.installResult.addons_dir;
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", {
      path: state.path || null,
    });
  });
}

function planSingleUpdate(target: string) {
  if (!target) return;

  return withLoading(async () => {
    state.singleUpdatePlan = await invoke<SingleUpdatePlanResponse>("plan_single_update", {
      target,
      path: effectiveAddonsPath(),
      force: state.forceUpdate,
    });
    state.singleUpdateResult = null;
    state.updateAllPlan = null;
    state.updateAllResult = null;
    state.path = state.singleUpdatePlan.addons_dir;
  });
}

function confirmUpdate() {
  const plan = state.singleUpdatePlan;
  if (!plan || !plan.should_install || !plan.plan) return;

  const backupText = plan.plan.items.some((item) => item.action === "would-replace-existing")
    ? "Existing folders may be backed up and replaced. A default timestamped backup folder will be created inside the AddOns directory unless the app is configured otherwise."
    : "No replacement is currently planned, so no backup folder is expected unless the fresh update plan changes.";
  const confirmed = window.confirm(
    `Update ${plan.local.folder_name}?\n\nFiles may be written to your AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n\nThe app will match the addon again, fetch fresh metadata, download and verify the ZIP, validate it, build a fresh plan, and back up replacements before applying.`,
  );
  if (!confirmed) return;

  return withLoading(async () => {
    state.singleUpdateResult = await invoke<SingleUpdateApplyResponse>("apply_single_update", {
      target: plan.target,
      path: effectiveAddonsPath(),
      backupDir: state.settings?.backup_dir_override || null,
      keepDownload: state.settings?.keep_downloads_default ?? false,
      downloadDir: state.settings?.download_dir || null,
      force: state.forceUpdate,
    });
    state.path = state.singleUpdateResult.addons_dir;
    state.updateAllPlan = null;
    state.updateAllResult = null;
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", {
      path: state.path || null,
    });
    const updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
      path: effectiveAddonsPath(),
      includeUnknown: updateIncludeUnknownDefault(),
    });
    state.updatePlan = updatePlan;
    state.updates = updatesFromPlan(updatePlan);
  });
}

function planUpdateAll() {
  return withLoading(async () => {
    state.updateAllPlan = await invoke<PlanUpdateAllResponse>("plan_update_all", {
      path: effectiveAddonsPath(),
      includeUnknown: updateIncludeUnknownDefault(),
      limit: null,
    });
    state.path = state.updateAllPlan.addons_dir;
    state.updateAllResult = null;
    state.singleUpdatePlan = null;
    state.singleUpdateResult = null;
  });
}

function applyUpdateAll() {
  const plan = state.updateAllPlan;
  if (!plan || plan.summary.planned_updates === 0) return;

  const confirmed = window.confirm(
    `Apply ${plan.summary.planned_updates} planned update${plan.summary.planned_updates === 1 ? "" : "s"}?\n\nFiles may be written to your AddOns directory:\n${plan.addons_dir}\n\nThe app will process updates sequentially, fetch fresh metadata for each addon, download and verify each ZIP, validate each package, and back up replacements before applying. It will stop on the first error.`,
  );
  if (!confirmed) return;

  return withLoading(async () => {
    state.updateAllResult = await invoke<ApplyUpdateAllResponse>("apply_update_all", {
      path: effectiveAddonsPath(),
      backupDir: state.settings?.backup_dir_override || null,
      keepDownload: state.settings?.keep_downloads_default ?? false,
      downloadDir: state.settings?.download_dir || null,
      includeUnknown: plan.include_unknown,
      limit: plan.limit,
    });
    state.path = state.updateAllResult.addons_dir;
    state.updateAllPlan = null;
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", {
      path: state.path || null,
    });
    const updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
      path: effectiveAddonsPath(),
      includeUnknown: updateIncludeUnknownDefault(),
    });
    state.updatePlan = updatePlan;
    state.updates = updatesFromPlan(updatePlan);
  });
}

function loadUpdates() {
  return withLoading(async () => {
    const updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
      path: effectiveAddonsPath(),
      includeUnknown: updateIncludeUnknownDefault(),
    });
    state.updatePlan = updatePlan;
    state.updates = updatesFromPlan(updatePlan);
    state.path = updatePlan.addons_dir;
    state.singleUpdatePlan = null;
    state.singleUpdateResult = null;
    state.updateAllPlan = null;
    state.updateAllResult = null;
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
loadSettings().then(loadInstalled);

function loadSettings() {
  return withLoading(async () => {
    state.settings = await invoke<AppSettings>("get_app_settings");
    applySettingsToState(state.settings);
    state.addonsPathExists = await invoke<boolean>("path_exists", {
      path: effectiveAddonsPath(),
    });
  });
}

function applySettingsToState(settings: AppSettings) {
  state.path = settings.addons_dir_override ?? "";
  state.includeUnknown = settings.include_unknown_updates_default;
}

function effectiveAddonsPath() {
  const value = state.path.trim();
  return value.length > 0 ? value : null;
}

function updateIncludeUnknownDefault() {
  return state.includeUnknown;
}

function syncSettingsDraft() {
  const next = readSettingsDraft();
  state.settings = next;
  state.addonsPathExists = null;
}

function readSettingsDraft(): AppSettings {
  const addonsDir = valueOrNull("#settings-addons-dir");
  const backupDir = valueOrNull("#settings-backup-dir");
  const downloadDir = valueOrNull("#settings-download-dir");
  const keepDownloads = checkedOrFalse("#settings-keep-downloads");
  const includeUnknown = checkedOrFalse("#settings-include-unknown");
  return {
    addons_dir_override: addonsDir,
    backup_dir_override: backupDir,
    download_dir: downloadDir,
    keep_downloads_default: keepDownloads,
    include_unknown_updates_default: includeUnknown,
  };
}

function saveSettings() {
  return withLoading(async () => {
    const saved = await invoke<AppSettings>("save_app_settings", {
      settings: readSettingsDraft() as AppSettingsInput,
    });
    state.settings = saved;
    applySettingsToState(saved);
    state.addonsPathExists = await invoke<boolean>("path_exists", {
      path: effectiveAddonsPath(),
    });
  });
}

function resetSettings() {
  return withLoading(async () => {
    const reset = await invoke<AppSettings>("reset_app_settings");
    state.settings = reset;
    applySettingsToState(reset);
    state.addonsPathExists = await invoke<boolean>("path_exists", {
      path: effectiveAddonsPath(),
    });
    render();
  });
}

function settingField(label: string, id: string, value: string) {
  return `
    <label class="setting-item" for="${escapeAttr(id)}">
      <span>${escapeHtml(label)}</span>
      <input id="${escapeAttr(id)}" value="${escapeAttr(value)}" placeholder="Leave blank for default" ${disabledAttr()} />
    </label>
  `;
}

function pathDisplay(value: string) {
  return `<p class="path-display" title="${escapeAttr(value)}">${escapeHtml(value)}</p>`;
}

function emptyRow(colspan: number, message: string) {
  return `
    <tr>
      <td class="empty-cell" colspan="${colspan}">${escapeHtml(message)}</td>
    </tr>
  `;
}

function disabledAttr() {
  return state.loading ? "disabled" : "";
}

function updatesFromPlan(plan: PlanUpdatesResponse): CheckAddonsResponse {
  return {
    addons_dir: plan.addons_dir,
    remote_addons_loaded: plan.remote_addons_loaded,
    matches: plan.matches,
  };
}

function settingCheckbox(label: string, id: string, value: boolean) {
  return `
    <label class="setting-item checkbox" for="${escapeAttr(id)}">
      <input type="checkbox" id="${escapeAttr(id)}" ${value ? "checked" : ""} ${disabledAttr()} />
      <span>${escapeHtml(label)}</span>
    </label>
  `;
}

function valueOrNull(selector: string) {
  return document.querySelector<HTMLInputElement>(selector)?.value.trim() || null;
}

function checkedOrFalse(selector: string) {
  return Boolean(document.querySelector<HTMLInputElement>(selector)?.checked);
}
