import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import iconSpriteUrl from "./assets/esoui/icons-45px.png";
import logoUrl from "./assets/LOGO.png";
import "./styles.css";
import type {
  AddonDetails,
  AddonSummary,
  AppSettings,
  AppSettingsInput,
  AppStartupInfo,
  ApplyUpdateAllResponse,
  CheckAddonsResponse,
  ImportExistingAddonsResponse,
  InstallRemoteAddonResponse,
  InstalledAddonsResponse,
  LocalAddon,
  MatchResult,
  PlanRemoteInstallResponse,
  PlanUpdateAllResponse,
  PlanUpdatesResponse,
  SearchResponse,
  SingleUpdateApplyResponse,
  SingleUpdatePlanResponse,
  UpdateAllAction,
} from "./types";

type Tab = "installed" | "search" | "settings";
type InstalledFilter = "all" | "update" | "unknown" | "current";
type InstalledSort = "name" | "updated" | "downloads" | "status";
type IconName = "check" | "folder" | "rotate" | "target";

interface AppState {
  tab: Tab;
  path: string;
  loading: boolean;
  error: string | null;
  warning: string | null;
  installed: InstalledAddonsResponse | null;
  searchQuery: string;
  installedQuery: string;
  installedFilter: InstalledFilter;
  installedSort: InstalledSort;
  searchLimit: number;
  searchResults: AddonSummary[];
  selectedDetails: AddonDetails | null;
  selectedLocal: LocalAddon | null;
  selectedMatch: MatchResult | null;
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
  needsInitialSetup: boolean;
  detectedAddonsPath: string | null;
  setupAddonsPath: string;
  setupImportPath: string | null;
  setupExistingAddonsCount: number;
}

interface InstalledViewModel {
  addon: LocalAddon;
  match: MatchResult | null;
}

interface CategoryMeta {
  name: string;
  x: number;
  y: number;
}

const state: AppState = {
  tab: "installed",
  path: "",
  loading: false,
  error: null,
  warning: null,
  installed: null,
  searchQuery: "",
  installedQuery: "",
  installedFilter: "all",
  installedSort: "status",
  searchLimit: 25,
  searchResults: [],
  selectedDetails: null,
  selectedLocal: null,
  selectedMatch: null,
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
  needsInitialSetup: false,
  detectedAddonsPath: null,
  setupAddonsPath: "",
  setupImportPath: null,
  setupExistingAddonsCount: 0,
};

const appRoot = document.querySelector<HTMLDivElement>("#app");

if (!appRoot) {
  throw new Error("missing app root");
}

const app = appRoot;

function render() {
  if (state.needsInitialSetup) {
    app.innerHTML = renderInitialSetup();
    bindInitialSetupEvents();
    return;
  }

  app.innerHTML = `
    <main class="app-shell">
      <aside class="sidebar">
        <div class="brand">
          ${brandMark()}
          <div>
            <h1>Scribe</h1>
            <p>ESO Addon Manager</p>
          </div>
        </div>
        <nav class="nav-list">
          ${tabButton("installed", "Installed")}
          ${tabButton("search", "Search")}
          ${tabButton("settings", "Settings")}
        </nav>
      </aside>
      <section class="content">
        ${state.error ? `<div class="banner error">${escapeHtml(state.error)}</div>` : ""}
        ${state.warning ? `<div class="banner warning">${escapeHtml(state.warning)}</div>` : ""}
        ${state.loading ? renderLoading() : ""}
        ${renderCurrentTab()}
        ${renderDetailsDrawer()}
      </section>
    </main>
  `;
  bindCommonEvents();
  bindTabEvents();
}

function renderInitialSetup() {
  return `
    <main class="setup-shell">
      <section class="setup-panel">
        <div class="brand setup-brand">
          ${brandMark()}
          <div>
            <h1>Scribe</h1>
            <p>ESO Addon Manager</p>
          </div>
        </div>
        ${state.error ? `<div class="banner error">${escapeHtml(state.error)}</div>` : ""}
        ${state.loading ? renderLoading() : ""}
        <div class="setup-heading">
          <h2>Select AddOns Path</h2>
          <p>Choose the ESO AddOns folder Scribe should manage.</p>
        </div>
        ${state.detectedAddonsPath ? `<div class="banner info">Detected AddOns path: ${escapeHtml(state.detectedAddonsPath)}</div>` : `<div class="banner warning">No ESO AddOns path was detected automatically.</div>`}
        <div class="field" id="setup-addons-field">
          <label for="setup-addons-dir">AddOns path</label>
          <div class="field-with-action">
            <input id="setup-addons-dir" value="${escapeAttr(state.setupAddonsPath)}" placeholder="C:\\Users\\Name\\Documents\\Elder Scrolls Online\\live\\AddOns" ${disabledAttr()} />
            <button class="secondary icon-button" id="browse-setup-addons" title="Browse for AddOns folder" ${disabledAttr()}>${icon("folder")} Browse</button>
          </div>
        </div>
        <div class="setup-actions">
          ${state.detectedAddonsPath ? `<button class="secondary icon-button" id="use-detected-addons" ${disabledAttr()}>${icon("target")} Use Detected</button>` : ""}
          <button class="primary icon-button" id="save-initial-setup" ${disabledAttr()}>${icon("check")} Continue</button>
        </div>
      </section>
      ${renderInitialImportModal()}
    </main>
  `;
}

function renderInitialImportModal() {
  if (!state.setupImportPath) return "";
  return `
    <div class="modal-backdrop" role="presentation">
      <section class="modal-panel" role="dialog" aria-modal="true" aria-labelledby="setup-import-title">
        <div class="modal-icon">${icon("target")}</div>
        <div class="modal-content">
          <h2 id="setup-import-title">Import Existing AddOns As Current</h2>
          <p>Scribe found ${state.setupExistingAddonsCount} existing addon${state.setupExistingAddonsCount === 1 ? "" : "s"} in this folder.</p>
          <p>Before continuing, update them with your current addon manager. Scribe cannot reliably verify every existing installed version.</p>
          <p>If you confirm, matched existing addons will be imported as up to date and used as the baseline for future updates.</p>
          <div class="modal-path" title="${escapeAttr(state.setupImportPath)}">${escapeHtml(state.setupImportPath)}</div>
        </div>
        <div class="modal-actions">
          <button class="secondary" id="cancel-initial-import" ${disabledAttr()}>Go Back</button>
          <button class="primary icon-button" id="confirm-initial-import" ${disabledAttr()}>${icon("check")} Import As Current</button>
        </div>
      </section>
    </div>
  `;
}

function renderCurrentTab() {
  if (state.tab === "installed") return renderInstalled();
  if (state.tab === "search") return renderSearch();
  return renderSettings();
}

function tabButton(tab: Tab, label: string) {
  return `<button class="nav-button ${state.tab === tab ? "active" : ""}" data-tab="${tab}">${escapeHtml(label)}</button>`;
}

function brandMark() {
  return `<span class="brand-mark"><img src="${escapeAttr(logoUrl)}" alt="" /></span>`;
}

function renderInstalled() {
  const view = installedView();
  return `
    ${pageHeader(
      "Installed Addons",
      "",
      `
        <button class="primary" id="plan-update-all-installed" ${disabledAttr()} ${state.updatePlan ? "" : "disabled"}>Plan All Updates</button>
        <button class="secondary" id="refresh-installed" ${disabledAttr()}>Refresh</button>
      `,
    )}
    ${state.addonsPathExists === false ? `<div class="banner warning compact-banner">Configured AddOns path is missing. <button class="link-button" id="open-settings">Open Settings</button></div>` : ""}
    <section class="control-panel">
      <label class="field">
        <span>Filter installed</span>
        <input id="installed-filter-input" value="${escapeAttr(state.installedQuery)}" placeholder="Addon name, author, folder" ${disabledAttr()} />
      </label>
      <label class="field sort-field">
        <span>Sort</span>
        <select id="installed-sort" ${disabledAttr()}>
          ${sortOption("status", "Update priority")}
          ${sortOption("name", "Name")}
          ${sortOption("updated", "Last updated")}
          ${sortOption("downloads", "Downloads")}
        </select>
      </label>
    </section>
    <section class="addon-list" id="installed-list">${renderInstalledList(view)}</section>
    ${renderUpdateAllPlan()}
    ${renderUpdateAllResult()}
    ${renderSingleUpdatePlan()}
    ${renderSingleUpdateResult()}
  `;
}

function renderInstalledList(view = installedView()) {
  if (state.loading && !state.installed) return renderSkeletonCards(5);
  if (view.length === 0) {
    return emptyState("No addons to show", state.installed ? "Try another filter or refresh this AddOns directory." : "Refresh to scan your AddOns directory.");
  }
  return view.map(renderInstalledCard).join("");
}

function renderInstalledCard(item: InstalledViewModel) {
  const addon = item.addon;
  const remote = item.match?.remote ?? null;
  const status = installedStatus(item.match, addon);
  const title = addon.folder_name;
  const category = categoryMeta(remote?.category_name ?? null, remote?.category_id ?? null, addon.is_library);
  const author = addon.author ?? remote?.author_name ?? null;
  const statusNote = installedStatusNote(status);
  return `
    <article class="addon-card clickable ${cardStatusClass(status.kind)}" data-installed-folder="${escapeAttr(addon.folder_name)}">
      ${CategoryIcon(category)}
      <div class="addon-main">
        <div class="addon-title-row">
          <div>
            <h3>${renderEsoText(title)}</h3>
            <p>${escapeHtml(installedSubtitle(addon, author, category.name))}</p>
          </div>
          ${statusNote ? `<span class="status-note">${escapeHtml(statusNote)}</span>` : ""}
        </div>
        <div class="meta-grid">
          ${metaItem("Installed", addon.display_version)}
          ${metaItem("Remote", remote?.version ?? null)}
          ${metaItem("Downloads", formatCount(remote?.downloads))}
          ${metaItem("Updated", remote?.updated_display ?? null)}
        </div>
      </div>
      <div class="card-actions">${renderCardUpdateAction(item.match)}</div>
    </article>
  `;
}

function installedSubtitle(addon: LocalAddon, author: string | null, category: string) {
  if (!addon.valid_manifest) {
    return `No valid addon manifest - folder ${addon.folder_name}`;
  }
  return `${author ? `by ${plainEsoText(author)}` : "Author unknown"} - ${category}`;
}

function renderSearch() {
  return `
    ${pageHeader("Search", "Find addons from remote metadata.", `<button class="primary" id="run-search" ${disabledAttr()}>Search</button>`)}
    <section class="control-panel search-controls">
      <label class="field">
        <span>Search term</span>
        <input id="search-query" value="${escapeAttr(state.searchQuery)}" placeholder="Addon name, author, or keyword" ${disabledAttr()} />
      </label>
      <label class="field limit-field">
        <span>Limit</span>
        <select id="search-limit" ${disabledAttr()}>
          ${[10, 25, 50, 100].map((value) => `<option value="${value}" ${state.searchLimit === value ? "selected" : ""}>${value}</option>`).join("")}
        </select>
      </label>
    </section>
    <section class="addon-list">
      ${
        state.loading && state.searchResults.length === 0
          ? renderSkeletonCards(4)
          : state.searchResults.length === 0
            ? emptyState("No search results", state.searchQuery.trim() ? "No remote addons matched this search." : "Enter a search term, then run Search.")
            : state.searchResults.map(renderSearchCard).join("")
      }
    </section>
  `;
}

function renderSearchCard(addon: AddonSummary) {
  const category = categoryMeta(addon.category_name, addon.category_id, false);
  return `
    <article class="addon-card clickable" data-addon-id="${escapeAttr(addon.uid ?? "")}">
      ${CategoryIcon(category)}
      <div class="addon-main">
        <div class="addon-title-row">
          <div>
            <h3>${renderEsoText(addon.name ?? "Unnamed addon")}</h3>
            <p>${escapeHtml(addon.author_name ? `by ${plainEsoText(addon.author_name)}` : "Author unknown")} &middot; ${escapeHtml(category.name)}</p>
          </div>
          ${addon.uid ? `<span class="mini-id">UID ${escapeHtml(addon.uid)}</span>` : ""}
        </div>
        <div class="meta-grid">
          ${metaItem("Version", addon.version)}
          ${metaItem("Downloads", formatCount(addon.downloads))}
          ${metaItem("Monthly", formatCount(addon.monthly_downloads))}
          ${metaItem("Updated", addon.updated_display)}
        </div>
      </div>
      <div class="card-actions"></div>
    </article>
  `;
}

function renderDrawerAction() {
  const details = state.selectedDetails;
  const match = state.selectedMatch;
  if (!state.selectedLocal && details?.uid) {
    return `<button class="primary" id="plan-install" ${disabledAttr()}>Plan Install</button>`;
  }
  if (match?.update_confidence === "reliable-update") {
    return `<button class="primary" data-plan-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>Plan Update</button>`;
  }
  if (state.forceUpdate && match && ["matched", "unknown-update", "local-newer"].includes(match.status)) {
    return `<button class="secondary" data-plan-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>Plan Reinstall</button>`;
  }
  return "";
}

function renderDetailsDrawer() {
  const details = state.selectedDetails;
  const local = state.selectedLocal;
  const match = state.selectedMatch;
  if (!details && !local) return "";
  const category = categoryMeta(details?.category_name ?? match?.remote?.category_name ?? null, details?.category_id ?? match?.remote?.category_id ?? null, local?.is_library ?? false);
  const title = details?.name ?? local?.title ?? match?.remote?.name ?? local?.folder_name ?? "Addon Details";
  const author = details?.author_name ?? match?.remote?.author_name ?? local?.author ?? null;
  const installedVersion = local?.display_version ?? null;
  const remoteVersion = details?.version ?? match?.remote?.version ?? null;
  const downloads = details?.downloads ?? match?.remote?.downloads ?? null;
  const updated = details?.updated_display ?? match?.remote?.updated_display ?? null;
  return `
    <div class="drawer-backdrop" id="close-details-backdrop"></div>
    <aside class="details-drawer" role="dialog" aria-modal="true" aria-label="Addon details">
      <button class="drawer-close" id="close-details" aria-label="Close details">Close</button>
      <section class="detail-hero">
        ${CategoryIcon(category, true)}
        <div>
          <h2>${renderEsoText(title)}</h2>
          <p>${escapeHtml(author ? `by ${plainEsoText(author)}` : "Author unknown")} &middot; ${escapeHtml(category.name)}</p>
          <div class="meta-grid detail-meta">
            ${metaItem("Installed", installedVersion)}
            ${metaItem("Remote", remoteVersion)}
            ${metaItem("Downloads", formatCount(downloads))}
            ${metaItem("Updated", updated)}
          </div>
        </div>
      </section>
      <div class="drawer-actions">
        ${renderDrawerAction()}
      </div>
      ${renderInstallPlan()}
      ${renderInstallResult()}
      ${renderSingleUpdatePlan()}
      ${renderSingleUpdateResult()}
      ${textBlock("Description", details?.description ?? match?.remote?.summary ?? null)}
      ${textBlock("Changelog", details?.changelog ?? null)}
      <details class="panel technical-details">
        <summary>Technical details</summary>
        <div class="details-grid">
          ${detailItem("UID", details?.uid ?? match?.remote?.uid ?? null)}
          ${detailItem("Filename", details?.file_name ?? null)}
          ${detailItem("MD5", details?.md5 ?? null)}
          ${detailItem("Info URL", details?.file_info_url ?? match?.remote?.file_info_url ?? null)}
          ${detailItem("Download URL", details?.download_url ?? null)}
          ${detailItem("Local folder", local?.folder_name ?? null)}
          ${detailItem("Local path", local?.folder_path ?? null)}
        </div>
      </details>
    </aside>
  `;
}

function renderSettings() {
  const settings = state.settings;
  const addonsMissing = Boolean(settings?.addons_dir_override) && state.addonsPathExists === false;
  return `
    ${pageHeader("Settings", "Desktop defaults for install and update actions.", `
      <div class="toolbar-actions">
        <button class="secondary icon-button" id="reset-settings" ${disabledAttr()}>${icon("rotate")} Reset</button>
        <button class="primary icon-button" id="save-settings" ${disabledAttr()}>${icon("check")} Save</button>
      </div>
    `)}
    ${addonsMissing ? `<div class="banner error">Configured AddOns path does not exist: ${escapeHtml(settings?.addons_dir_override ?? "")}</div>` : ""}
    <div class="banner info">Blank paths use auto-detection or built-in defaults. Backup and download directories are created only when an install or update writes files there.</div>
    <section class="settings-grid">
      ${settingField("AddOns path override", "settings-addons-dir", settings?.addons_dir_override ?? "", true)}
      ${settingField("Backup directory override", "settings-backup-dir", settings?.backup_dir_override ?? "", true)}
      ${settingField("Download directory", "settings-download-dir", settings?.download_dir ?? "", true)}
      ${settingCheckbox("Keep downloads default", "settings-keep-downloads", settings?.keep_downloads_default ?? false)}
      ${settingCheckbox("Include unknown updates default", "settings-include-unknown", settings?.include_unknown_updates_default ?? false)}
    </section>
  `;
}

function renderInstallPlan() {
  const plan = state.installPlan;
  if (!plan) return "";
  return `
    <section class="panel">
      <div class="banner info">Dry run only. This preview downloaded and validated the ZIP, but did not change your AddOns directory.</div>
      <div class="panel-heading">
        <div>
          <h3>Install Preview</h3>
          <p>Review this plan before continuing.</p>
        </div>
        <button class="danger" id="confirm-install" ${disabledAttr()}>Install</button>
      </div>
      ${renderPlanItems(plan.plan.items)}
    </section>
  `;
}

function renderInstallResult() {
  const result = state.installResult;
  if (!result) return "";
  return `
    <section class="panel">
      <div class="banner ${result.applied ? "success" : "warning"}">Install ${result.applied ? "completed" : "finished without file changes"}.</div>
      <div class="summary">
        ${summaryItem("Installed", result.installed_new)}
        ${summaryItem("Replaced", result.replaced)}
        ${summaryItem("Skipped", result.skipped)}
        ${summaryItem("Applied", result.applied ? 1 : 0)}
      </div>
      ${renderInstalledItems(result.items)}
      <button class="primary" id="refresh-installed-after-install" ${disabledAttr()}>Refresh Installed</button>
    </section>
  `;
}

function renderUpdateAllPlan() {
  const plan = state.updateAllPlan;
  if (!plan) return "";
  return `
    <section class="panel">
      <div class="banner info">Dry run only. This plan did not download, extract, modify, or delete addon files.</div>
      <div class="panel-heading">
        <div>
          <h3>All Updates Preview</h3>
          <p>${plan.summary.planned_updates} planned update${plan.summary.planned_updates === 1 ? "" : "s"} in ${escapeHtml(plan.addons_dir)}</p>
        </div>
        ${plan.summary.planned_updates > 0 ? `<button class="danger" id="apply-update-all" ${disabledAttr()}>Apply All Updates</button>` : ""}
      </div>
      <div class="addon-list">
        ${
          plan.actions.length === 0
            ? emptyState("No update candidates", "No update candidates were found in this plan.")
            : plan.actions.map(renderUpdateAllActionCard).join("")
        }
      </div>
    </section>
  `;
}

function renderUpdateAllActionCard(action: UpdateAllAction) {
  const icon = action.update_all_action === "would-update" ? categoryIconByKey["developer-utilities"] : categoryIconByKey.misc;
  return `
    <article class="addon-card compact-card">
      ${CategoryIcon({ ...icon, name: action.update_all_action === "would-update" ? "Update" : "Skipped" })}
      <div class="addon-main">
        <div class="addon-title-row">
          <div>
            <h3>${renderEsoText(action.local_folder)}</h3>
            <p>${renderEsoText(action.remote_name ?? "No remote match")}</p>
          </div>
          ${statusBadge(action.update_all_action === "would-update" ? "Will update" : "Skipped", action.update_all_action === "would-update" ? "update" : "neutral")}
        </div>
        <div class="meta-grid">
          ${metaItem("Installed", action.local_version)}
          ${metaItem("Remote", action.remote_version)}
          ${metaItem("Plan", action.action)}
          ${metaItem("Update-all", action.update_all_action)}
        </div>
      </div>
    </article>
  `;
}

function renderUpdateAllResult() {
  const result = state.updateAllResult;
  if (!result) return "";
  return `
    <section class="panel">
      <div class="banner ${result.applied ? "success" : "warning"}">Update-all ${result.applied ? "completed" : "finished without file changes"}.</div>
      <div class="summary">
        ${summaryItem("Updated", result.results.length)}
        ${summaryItem("Planned", result.summary.planned_updates)}
        ${summaryItem("Applied", result.applied ? 1 : 0)}
      </div>
      ${result.results.length === 0 ? emptyState("No updates applied", "No addons were updated.") : result.results.map(renderUpdateAllResultCard).join("")}
    </section>
  `;
}

function renderUpdateAllResultCard(item: ApplyUpdateAllResponse["results"][number]) {
  return `
    <article class="addon-card compact-card">
      ${CategoryIcon(categoryMeta(item.remote_details.category_name, item.remote_details.category_id, false))}
      <div class="addon-main">
        <div class="addon-title-row">
          <div>
            <h3>${renderEsoText(item.target.local_folder)}</h3>
            <p>${renderEsoText(item.remote_details.name ?? item.target.remote_name ?? "Updated addon")}</p>
          </div>
          ${statusBadge("Updated", "current")}
        </div>
        <div class="meta-grid">
          ${metaItem("Installed", String(item.installed_new))}
          ${metaItem("Replaced", String(item.replaced))}
          ${metaItem("Skipped", String(item.skipped))}
          ${metaItem("Backup", item.backup_dir)}
        </div>
      </div>
    </article>
  `;
}

function renderSingleUpdatePlan() {
  const plan = state.singleUpdatePlan;
  if (!plan) return "";
  return `
    <section class="panel">
      <div class="panel-heading">
        <div>
          <h3>Single Update Preview</h3>
          <p>${escapeHtml(plan.reason ?? plan.decision)}</p>
        </div>
        ${plan.should_install ? `<button class="danger" id="confirm-update" ${disabledAttr()}>Apply Update</button>` : ""}
      </div>
      ${plan.plan ? renderPlanItems(plan.plan.items) : emptyState("No file changes planned", "This addon is not eligible for update with the current options.")}
    </section>
  `;
}

function renderSingleUpdateResult() {
  const result = state.singleUpdateResult;
  if (!result) return "";
  return `
    <section class="panel">
      <div class="banner ${result.applied ? "success" : "warning"}">Update ${result.applied ? "completed" : "finished without file changes"}.</div>
      <div class="summary">
        ${summaryItem("Installed", result.installed_new)}
        ${summaryItem("Replaced", result.replaced)}
        ${summaryItem("Skipped", result.skipped)}
        ${summaryItem("Applied", result.applied ? 1 : 0)}
      </div>
      ${renderInstalledItems(result.items)}
    </section>
  `;
}

function renderPlanItems(items: { source_folder: string | null; target_folder: string | null; action: string; title: string | null; version: string | null }[]) {
  if (items.length === 0) return emptyState("No plan items", "No addon folders were found in this preview.");
  return `
    <div class="mini-list">
      ${items
        .map(
          (item) => `
            <div class="mini-row">
              <strong>${renderEsoText(item.title ?? item.source_folder ?? "Unknown")}</strong>
              <span>${escapeHtml(item.version ?? "-")}</span>
              <span>${escapeHtml(item.action)} ${item.target_folder ? `-> ${escapeHtml(item.target_folder)}` : ""}</span>
            </div>
          `,
        )
        .join("")}
    </div>
  `;
}

function renderInstalledItems(items: { target_folder: string | null; action: string; message: string | null }[]) {
  if (items.length === 0) return emptyState("No result items", "No addon folders were reported.");
  return `
    <div class="mini-list">
      ${items
        .map(
          (item) => `
            <div class="mini-row">
              <strong>${escapeHtml(item.target_folder ?? "Unknown")}</strong>
              <span>${escapeHtml(item.action)}</span>
              <span>${escapeHtml(item.message ?? "-")}</span>
            </div>
          `,
        )
        .join("")}
    </div>
  `;
}

function bindCommonEvents() {
  document.querySelectorAll<HTMLButtonElement>("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.tab = button.dataset.tab as Tab;
      state.error = null;
      state.warning = null;
      render();
    });
  });
}

function bindInitialSetupEvents() {
  document.querySelector<HTMLInputElement>("#setup-addons-dir")?.addEventListener("input", (event) => {
    state.setupAddonsPath = (event.currentTarget as HTMLInputElement).value;
    clearPendingInitialImport();
  });
  document.querySelector<HTMLButtonElement>("#browse-setup-addons")?.addEventListener("click", browseInitialSetupFolder);
  document.querySelector<HTMLButtonElement>("#use-detected-addons")?.addEventListener("click", () => {
    state.setupAddonsPath = state.detectedAddonsPath ?? "";
    clearPendingInitialImport();
    state.error = null;
    render();
  });
  document.querySelector<HTMLButtonElement>("#save-initial-setup")?.addEventListener("click", saveInitialSetup);
  document.querySelector<HTMLButtonElement>("#cancel-initial-import")?.addEventListener("click", () => {
    clearPendingInitialImport();
    render();
  });
  document.querySelector<HTMLButtonElement>("#confirm-initial-import")?.addEventListener("click", confirmInitialImport);
}

function bindTabEvents() {
  document.querySelector<HTMLButtonElement>("#refresh-installed")?.addEventListener("click", loadInstalled);
  document.querySelector<HTMLButtonElement>("#plan-update-all-installed")?.addEventListener("click", planUpdateAll);
  document.querySelector<HTMLButtonElement>("#open-settings")?.addEventListener("click", () => {
    state.tab = "settings";
    render();
  });
  document.querySelector<HTMLInputElement>("#installed-filter-input")?.addEventListener("input", (event) => {
    state.installedQuery = (event.currentTarget as HTMLInputElement).value;
    renderInstalledListOnly();
  });
  document.querySelector<HTMLSelectElement>("#installed-sort")?.addEventListener("change", (event) => {
    state.installedSort = (event.currentTarget as HTMLSelectElement).value as InstalledSort;
    render();
  });
  document.querySelector<HTMLInputElement>("#search-query")?.addEventListener("input", (event) => {
    state.searchQuery = (event.currentTarget as HTMLInputElement).value;
  });
  document.querySelector<HTMLSelectElement>("#search-limit")?.addEventListener("change", (event) => {
    state.searchLimit = Number((event.currentTarget as HTMLSelectElement).value);
  });
  document.querySelector<HTMLButtonElement>("#run-search")?.addEventListener("click", runSearch);
  document.querySelectorAll<HTMLElement>(".clickable").forEach((card) => {
    card.addEventListener("click", () => {
      if (card.dataset.installedFolder) openInstalledDetails(card.dataset.installedFolder);
      if (card.dataset.addonId) loadDetails(card.dataset.addonId);
    });
  });
  document.querySelectorAll<HTMLElement>("button, a, input, select, summary").forEach((element) => {
    element.addEventListener("click", (event) => event.stopPropagation());
  });
  document.querySelector<HTMLButtonElement>("#close-details")?.addEventListener("click", closeDetails);
  document.querySelector<HTMLDivElement>("#close-details-backdrop")?.addEventListener("click", closeDetails);
  document.querySelector<HTMLButtonElement>("#plan-install")?.addEventListener("click", planInstall);
  document.querySelector<HTMLButtonElement>("#confirm-install")?.addEventListener("click", confirmInstall);
  document.querySelector<HTMLButtonElement>("#refresh-installed-after-install")?.addEventListener("click", loadInstalled);
  document.querySelector<HTMLButtonElement>("#apply-update-all")?.addEventListener("click", applyUpdateAll);
  document.querySelectorAll<HTMLButtonElement>("[data-plan-update-target]").forEach((button) => {
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      planSingleUpdate(button.dataset.planUpdateTarget ?? "");
    });
  });
  document.querySelector<HTMLButtonElement>("#confirm-update")?.addEventListener("click", confirmUpdate);
  document.querySelector<HTMLButtonElement>("#save-settings")?.addEventListener("click", saveSettings);
  document.querySelector<HTMLButtonElement>("#reset-settings")?.addEventListener("click", resetSettings);
  document.querySelector<HTMLInputElement>("#settings-addons-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-backup-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-download-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-keep-downloads")?.addEventListener("change", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-include-unknown")?.addEventListener("change", syncSettingsDraft);
  document.querySelectorAll<HTMLButtonElement>("[data-browse-target]").forEach((button) => {
    button.addEventListener("click", () => browseSettingsFolder(button.dataset.browseTarget ?? ""));
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

function renderInstalledListOnly() {
  const list = document.querySelector<HTMLElement>("#installed-list");
  if (!list) return;
  list.innerHTML = renderInstalledList();
  bindCardEventsOnly();
}

function bindCardEventsOnly() {
  document.querySelectorAll<HTMLElement>(".clickable").forEach((card) => {
    card.addEventListener("click", () => {
      if (card.dataset.installedFolder) openInstalledDetails(card.dataset.installedFolder);
      if (card.dataset.addonId) loadDetails(card.dataset.addonId);
    });
  });
  document.querySelectorAll<HTMLElement>(".addon-card button, .addon-card a").forEach((element) => {
    element.addEventListener("click", (event) => event.stopPropagation());
  });
  document.querySelectorAll<HTMLButtonElement>("[data-plan-update-target]").forEach((button) => {
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      planSingleUpdate(button.dataset.planUpdateTarget ?? "");
    });
  });
}

async function browseInitialSetupFolder() {
  const selected = await browseForFolder(state.setupAddonsPath || state.detectedAddonsPath);
  if (!selected) return;
  state.setupAddonsPath = selected;
  clearPendingInitialImport();
  state.error = null;
  render();
}

async function browseSettingsFolder(targetId: string) {
  const input = document.querySelector<HTMLInputElement>(`#${targetId}`);
  if (!input) return;
  const selected = await browseForFolder(input.value || state.detectedAddonsPath);
  if (!selected) return;
  input.value = selected;
  state.error = null;
  syncSettingsDraft();
  render();
}

async function browseForFolder(defaultPath: string | null | undefined) {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
      defaultPath: defaultPath || undefined,
    });

    return typeof selected === "string" ? selected : null;
  } catch (error) {
    state.error = `Could not open folder browser. ${error instanceof Error ? error.message : String(error)}`;
    render();
    return null;
  }
}

function loadInstalled() {
  return withLoading(async () => {
    if (!state.settings) {
      state.settings = await invoke<AppSettings>("get_app_settings");
      applySettingsToState(state.settings);
    }
    await refreshInstalledData();
  });
}

async function refreshInstalledData() {
  state.addonsPathExists = await invoke<boolean>("path_exists", { path: effectiveAddonsPath() });
  state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: effectiveAddonsPath() });
  state.path = state.installed.addons_dir;
  try {
    const updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
      path: effectiveAddonsPath(),
      includeUnknown: updateIncludeUnknownDefault(),
    });
    state.updatePlan = updatePlan;
    state.updates = updatesFromPlan(updatePlan);
    state.warning = null;
  } catch (error) {
    state.updatePlan = null;
    state.updates = null;
    state.warning = `Remote metadata could not be loaded. Showing local addons only. ${error instanceof Error ? error.message : String(error)}`;
  }
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
  return withLoading(async () => {
    state.selectedDetails = await invoke<AddonDetails>("get_remote_addon_details", { addonId });
    state.selectedLocal = null;
    state.selectedMatch = null;
    state.installPlan = null;
    state.installResult = null;
  });
}

function openInstalledDetails(folderName: string) {
  const addon = state.installed?.addons.find((item) => item.folder_name === folderName) ?? null;
  const match = state.updates?.matches.find((item) => item.local.folder_name === folderName) ?? null;
  if (!addon) return;
  state.selectedLocal = addon;
  state.selectedMatch = match;
  state.selectedDetails = null;
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  render();
  const uid = match?.remote?.uid;
  if (uid) {
    withLoading(async () => {
      state.selectedDetails = await invoke<AddonDetails>("get_remote_addon_details", { addonId: uid });
    });
  }
}

function closeDetails() {
  state.selectedDetails = null;
  state.selectedLocal = null;
  state.selectedMatch = null;
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  render();
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
    ? "Existing addon folders may be backed up and replaced."
    : "No existing addon folder replacement is currently planned.";
  const confirmed = window.confirm(
    `Install ${plan.remote.name ?? addonId}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n\nThe app will fetch fresh metadata, download and verify the ZIP, validate it, build a fresh plan, and back up replacements before applying.`,
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
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: state.path || null });
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
    ? "Existing addon folders may be backed up and replaced."
    : "No existing addon folder replacement is currently planned.";
  const confirmed = window.confirm(
    `Update ${plan.local.folder_name}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n\nThe app will match the addon again, fetch fresh metadata, download and verify the ZIP, validate it, build a fresh plan, and back up replacements before applying.`,
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
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: state.path || null });
    await refreshUpdatePlan();
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
    `Apply ${plan.summary.planned_updates} planned update${plan.summary.planned_updates === 1 ? "" : "s"}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\nThe app will process updates sequentially, fetch fresh metadata for each addon, download and verify each ZIP, validate each package, and back up replacements before applying. It will stop on the first error.`,
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
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: state.path || null });
    await refreshUpdatePlan();
  });
}

function loadUpdates() {
  return withLoading(async () => {
    await refreshUpdatePlan();
    state.singleUpdatePlan = null;
    state.singleUpdateResult = null;
    state.updateAllPlan = null;
    state.updateAllResult = null;
  });
}

async function refreshUpdatePlan() {
  const updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
    path: effectiveAddonsPath(),
    includeUnknown: updateIncludeUnknownDefault(),
  });
  state.updatePlan = updatePlan;
  state.updates = updatesFromPlan(updatePlan);
  state.path = updatePlan.addons_dir;
}

function installedView(): InstalledViewModel[] {
  const matches = state.updates?.matches ?? [];
  const query = state.installedQuery.trim().toLowerCase();
  const items = (state.installed?.addons ?? []).map((addon) => ({
    addon,
    match: matches.find((match) => match.local.folder_name === addon.folder_name) ?? null,
  }));

  return items
    .filter((item) => {
      const status = installedStatus(item.match, item.addon);
      if (state.installedFilter === "update" && status.kind !== "update") return false;
      if (state.installedFilter === "current" && status.kind !== "current") return false;
      if (state.installedFilter === "unknown" && !["unknown", "not-found", "ambiguous"].includes(status.kind)) return false;
      if (!query) return true;
      return [item.addon.title, item.addon.folder_name, item.addon.author, item.match?.remote?.name, item.match?.remote?.author_name]
        .filter(Boolean)
        .join(" ")
        .replace(/\|c[0-9a-fA-F]{6,8}|\|r/g, "")
        .toLowerCase()
        .includes(query);
    })
    .sort(compareInstalled);
}

function compareInstalled(left: InstalledViewModel, right: InstalledViewModel) {
  if (state.installedSort === "downloads") {
    return (right.match?.remote?.downloads ?? -1) - (left.match?.remote?.downloads ?? -1);
  }
  if (state.installedSort === "updated") {
    return dateValue(right.match?.remote?.updated_display) - dateValue(left.match?.remote?.updated_display);
  }
  if (state.installedSort === "status") {
    const rank = installedStatus(left.match, left.addon).rank - installedStatus(right.match, right.addon).rank;
    return rank || displayName(left).localeCompare(displayName(right));
  }
  return displayName(left).localeCompare(displayName(right));
}

function displayName(item: InstalledViewModel) {
  return plainEsoText(item.addon.title ?? item.addon.folder_name);
}

function dateValue(value: string | null | undefined) {
  return value ? Date.parse(value) || 0 : 0;
}

function installedStatus(match: MatchResult | null, addon: LocalAddon) {
  if (!addon.valid_manifest) return { label: "Invalid folder", kind: "invalid", rank: 3 };
  if (match?.update_confidence === "current") return { label: "Current", kind: "current", rank: 4 };
  if (addon.is_library === true) return { label: "Unknown", kind: "unknown", rank: 2 };
  if (!match) return { label: "Unknown", kind: "unknown", rank: 2 };
  if (match.update_confidence === "reliable-update") return { label: "Update candidate", kind: "reliable-update", rank: 1 };
  if (match.update_confidence === "possible-update") return { label: "Version check uncertain", kind: "possible-update", rank: 2 };
  if (match.update_confidence === "local-newer") return { label: "Local newer", kind: "local-newer", rank: 5 };
  if (match.status === "possible-update") return { label: "Version differs", kind: "possible-update", rank: 2 };
  if (match.status === "unknown-update") return { label: "Unknown", kind: "unknown", rank: 2 };
  if (match.status === "no-match") return { label: "Not found", kind: "not-found", rank: 3 };
  if (match.status === "ambiguous") return { label: "Ambiguous", kind: "ambiguous", rank: 3 };
  if (match.status === "matched") return { label: "Current", kind: "current", rank: 4 };
  if (match.status === "local-newer") return { label: "Local newer", kind: "local-newer", rank: 5 };
  return { label: "Unknown", kind: "unknown", rank: 2 };
}

function renderCardUpdateAction(match: MatchResult | null) {
  if (!match) return "";
  if (match.update_confidence === "reliable-update") {
    return `<button class="primary small" data-plan-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>Plan Update</button>`;
  }
  if (state.forceUpdate && ["matched", "unknown-update", "local-newer"].includes(match.status)) {
    return `<button class="secondary small" data-plan-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>Plan Reinstall</button>`;
  }
  return "";
}

function installedStatusNote(status: { label: string; kind: string }) {
  if (status.kind === "reliable-update") return "Remote version differs";
  if (status.kind === "possible-update" || status.kind === "unknown") return "Version check uncertain";
  if (status.kind === "invalid") return "No valid manifest";
  if (status.kind === "not-found") return "Remote match not found";
  if (status.kind === "ambiguous") return "Remote match ambiguous";
  if (status.kind === "local-newer") return "Local newer";
  return "";
}

function cardStatusClass(kind: string) {
  if (kind === "reliable-update") return "is-update-candidate";
  if (["possible-update", "unknown", "not-found", "ambiguous", "local-newer", "invalid"].includes(kind)) {
    return "is-uncertain";
  }
  return "";
}

// ESOUI atlas is a 3-column grid. Icons render at 45x45; the image has a 50px pitch
// between category cells, so positions advance by 50px to avoid clipping into padding.
function spriteCell(index: number, name: string) {
  return {
    name,
    x: -((index % 3) * 50),
    y: -(Math.floor(index / 3) * 50),
  };
}

const categoryIconByKey: Record<string, { name: string; x: number; y: number }> = {
  "action-bar": spriteCell(0, "Action Bar Mods"),
  "auction-vendors": spriteCell(1, "Action House & Vendors"),
  "bags-bank-inventory": spriteCell(2, "Bags, Bank, Inventory"),
  "buff-debuff-spell": spriteCell(3, "Buff, Debuff, Spell"),
  "casting-cooldowns": spriteCell(4, "Casting Bars, Cooldowns"),
  "character-advancement": spriteCell(5, "Character Advancement"),
  chat: spriteCell(6, "Chat Mods"),
  "class-role": spriteCell(7, "Class & Role Specific"),
  combat: spriteCell(8, "Combat Mods"),
  data: spriteCell(9, "Data Mods"),
  "game-controller": spriteCell(10, "Game Controller"),
  "graphic-ui": spriteCell(11, "Graphic UI Mods"),
  "group-guild-friends": spriteCell(12, "Group, Guild & Friends"),
  homestead: spriteCell(13, "Homestead"),
  "info-bars": spriteCell(14, "Info, Plug-in Bars"),
  map: spriteCell(15, "Map, Coords, Compasses"),
  mail: spriteCell(16, "Mail"),
  pvp: spriteCell(17, "PvP"),
  raid: spriteCell(18, "Raid Mods"),
  roleplay: spriteCell(19, "RolePlay"),
  tradeskill: spriteCell(20, "TradeSkill Mods"),
  tooltip: spriteCell(21, "ToolTip"),
  "ui-media": spriteCell(22, "UI Media"),
  unit: spriteCell(23, "Unit Mods"),
  misc: spriteCell(24, "Miscellaneous"),
  utility: spriteCell(25, "Utility Mods"),
  libraries: spriteCell(26, "Libraries"),
  "developer-utilities": spriteCell(27, "Developer Utilities"),
  "eso-tools": spriteCell(28, "ESO Tools & Utilities"),
  "unofficial-translations": spriteCell(29, "Unofficial game translations"),
  beta: spriteCell(30, "Beta-version AddOns"),
  "plugins-patches": spriteCell(31, "Plug-Ins & Patches"),
  discontinued: spriteCell(32, "Discontinued & Outdated"),
};

const categoryKeyById: Record<string, string> = {
  "17": "graphic-ui",
  "18": "character-advancement",
  "19": "action-bar",
  "20": "bags-bank-inventory",
  "21": "unit",
  "22": "buff-debuff-spell",
  "24": "map",
  "25": "combat",
  "26": "data",
  "27": "misc",
  "33": "plugins-patches",
  "40": "tradeskill",
  "45": "raid",
  "53": "libraries",
  "55": "chat",
  "56": "class-role",
  "57": "class-role",
  "58": "class-role",
  "94": "auction-vendors",
  "95": "group-guild-friends",
  "96": "pvp",
  "97": "mail",
  "98": "tooltip",
  "109": "info-bars",
  "112": "casting-cooldowns",
  "114": "roleplay",
  "147": "ui-media",
  "149": "class-role",
  "150": "class-role",
  "151": "class-role",
  "152": "class-role",
  "155": "beta",
  "159": "utility",
  "160": "homestead",
  "162": "game-controller",
  "163": "unofficial-translations",
  "164": "class-role",
  "165": "class-role",
  "166": "class-role",
};

function CategoryIcon(category: CategoryMeta, large = false) {
  return `
    <div class="category-token ${large ? "large" : ""}" title="${escapeAttr(category.name)}" aria-label="${escapeAttr(category.name)}">
      <span class="category-icon" style="--icon-sprite: url('${escapeAttr(iconSpriteUrl)}'); --icon-x: ${category.x}px; --icon-y: ${category.y}px" aria-hidden="true"></span>
    </div>
  `;
}

function categoryMeta(name: string | null, id: string | null, isLibrary: boolean | null): CategoryMeta {
  const normalizedName = normalizeCategoryKey(name);
  const normalizedId = normalizeCategoryKey(id);
  const keyFromId = id ? categoryKeyById[id.trim()] : undefined;
  const key = isLibrary ? "libraries" : keyFromId ?? categoryKeyByName(normalizedName, normalizedId);
  const icon = categoryIconByKey[key] ?? categoryIconByKey.misc;
  return {
    ...icon,
    name: name?.trim() || icon.name,
  };
}

function categoryKeyByName(name: string, id: string) {
  const value = `${name} ${id}`;
  if (value.includes("action") && value.includes("bar")) return "action-bar";
  if (value.includes("auction") || value.includes("vendor")) return "auction-vendors";
  if (value.includes("bag") || value.includes("bank") || value.includes("inventory")) return "bags-bank-inventory";
  if (value.includes("buff") || value.includes("debuff") || value.includes("spell")) return "buff-debuff-spell";
  if (value.includes("casting") || value.includes("cooldown")) return "casting-cooldowns";
  if (value.includes("character") || value.includes("advancement") || value.includes("level")) return "character-advancement";
  if (value.includes("chat")) return "chat";
  if (value.includes("class") || value.includes("role specific")) return "class-role";
  if (value.includes("combat")) return "combat";
  if (value.includes("data")) return "data";
  if (value.includes("controller") || value.includes("gamepad")) return "game-controller";
  if (value.includes("graphic") || value.includes("interface") || value.includes("ui mod")) return "graphic-ui";
  if (value.includes("group") || value.includes("guild") || value.includes("friend") || value.includes("trad")) return "group-guild-friends";
  if (value.includes("homestead")) return "homestead";
  if (value.includes("info") || value.includes("plug in bar")) return "info-bars";
  if (value.includes("map") || value.includes("coord") || value.includes("compass") || value.includes("quest")) return "map";
  if (value.includes("mail")) return "mail";
  if (value.includes("pvp") || value.includes("alliance")) return "pvp";
  if (value.includes("raid") || value.includes("trial")) return "raid";
  if (value.includes("roleplay") || value.includes("role play") || value.includes("rp")) return "roleplay";
  if (value.includes("trade") || value.includes("craft")) return "tradeskill";
  if (value.includes("tooltip") || value.includes("tool tip")) return "tooltip";
  if (value.includes("media")) return "ui-media";
  if (value.includes("unit") || value.includes("frame")) return "unit";
  if (value.includes("utility")) return "utility";
  if (value.includes("librar")) return "libraries";
  if (value.includes("developer")) return "developer-utilities";
  if (value.includes("eso tool")) return "eso-tools";
  if (value.includes("translation") || value.includes("language")) return "unofficial-translations";
  if (value.includes("beta")) return "beta";
  if (value.includes("plugin") || value.includes("patch")) return "plugins-patches";
  if (value.includes("discontinued") || value.includes("outdated")) return "discontinued";
  return "misc";
}

function normalizeCategoryKey(value: string | null) {
  return (value ?? "")
    .toLowerCase()
    .replace(/&/g, "and")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function pageHeader(title: string, subtitle: string, actions: string) {
  return `
    <header class="page-header">
      <div>
        <h2>${escapeHtml(title)}</h2>
        ${subtitle ? pathDisplay(subtitle) : ""}
      </div>
      ${actions ? `<div class="toolbar-actions">${actions}</div>` : ""}
    </header>
  `;
}

function sortOption(value: InstalledSort, label: string) {
  return `<option value="${value}" ${state.installedSort === value ? "selected" : ""}>${escapeHtml(label)}</option>`;
}

function statusBadge(label: string, kind: string) {
  return `<span class="status-badge ${escapeAttr(kind)}">${escapeHtml(label)}</span>`;
}

function metaItem(label: string, value: string | number | null | undefined) {
  return `
    <div class="meta-item">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value === null || value === undefined || value === "" ? "-" : String(value))}</strong>
    </div>
  `;
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
    <section class="panel text-block">
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

function emptyState(title: string, message: string) {
  return `
    <div class="empty-state">
      <strong>${escapeHtml(title)}</strong>
      <p>${escapeHtml(message)}</p>
    </div>
  `;
}

function renderLoading() {
  return `<div class="banner info">Working...</div>`;
}

function renderSkeletonCards(count: number) {
  return Array.from({ length: count }, () => `<div class="skeleton-card"><span></span><div></div><p></p></div>`).join("");
}

function pathDisplay(value: string) {
  return `<p class="path-display" title="${escapeAttr(value)}">${escapeHtml(value)}</p>`;
}

function formatCount(value: number | null | undefined) {
  return value === null || value === undefined ? "-" : new Intl.NumberFormat().format(value);
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

function loadStartup() {
  return withLoading(async () => {
    const startup = await invoke<AppStartupInfo>("get_startup_info");
    state.settings = startup.settings;
    state.detectedAddonsPath = startup.detected_addons_dir;
    state.needsInitialSetup = !startup.settings_exists;
    state.setupAddonsPath = startup.settings.addons_dir_override ?? startup.detected_addons_dir ?? "";
    applySettingsToState(startup.settings);
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
  state.settings = readSettingsDraft();
  state.addonsPathExists = null;
}

function readSettingsDraft(): AppSettings {
  return {
    addons_dir_override: valueOrNull("#settings-addons-dir"),
    backup_dir_override: valueOrNull("#settings-backup-dir"),
    download_dir: valueOrNull("#settings-download-dir"),
    keep_downloads_default: checkedOrFalse("#settings-keep-downloads"),
    include_unknown_updates_default: checkedOrFalse("#settings-include-unknown"),
  };
}

function saveInitialSetup() {
  const selectedPath = state.setupAddonsPath.trim();
  if (!selectedPath) {
    state.error = "Enter an ESO AddOns path before continuing.";
    render();
    return;
  }

  return withLoading(async () => {
    const exists = await invoke<boolean>("path_exists", { path: selectedPath });
    if (!exists) {
      throw new Error(`Selected AddOns path does not exist: ${selectedPath}`);
    }

    const installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: selectedPath });
    const existingAddonsCount = installed.addons.filter((addon) => addon.valid_manifest).length;
    if (existingAddonsCount > 0) {
      state.setupImportPath = selectedPath;
      state.setupExistingAddonsCount = existingAddonsCount;
      return;
    }

    await finishInitialSetup(selectedPath, false);
  });
}

function confirmInitialImport() {
  const selectedPath = state.setupImportPath;
  if (!selectedPath) return;
  return withLoading(async () => {
    await finishInitialSetup(selectedPath, true);
  });
}

async function finishInitialSetup(selectedPath: string, importExisting: boolean) {
  if (importExisting) {
    await invoke<ImportExistingAddonsResponse>("import_existing_addons_as_current", {
      path: selectedPath,
    });
  }

  const saved = await invoke<AppSettings>("save_app_settings", {
    settings: {
      addons_dir_override: selectedPath,
      backup_dir_override: state.settings?.backup_dir_override ?? null,
      download_dir: state.settings?.download_dir ?? null,
      keep_downloads_default: state.settings?.keep_downloads_default ?? false,
      include_unknown_updates_default: state.settings?.include_unknown_updates_default ?? false,
    } satisfies AppSettingsInput,
  });
  state.settings = saved;
  state.needsInitialSetup = false;
  clearPendingInitialImport();
  applySettingsToState(saved);
  await refreshInstalledData();
}

function clearPendingInitialImport() {
  state.setupImportPath = null;
  state.setupExistingAddonsCount = 0;
}

function saveSettings() {
  return withLoading(async () => {
    const saved = await invoke<AppSettings>("save_app_settings", {
      settings: readSettingsDraft() as AppSettingsInput,
    });
    state.settings = saved;
    applySettingsToState(saved);
    state.addonsPathExists = await invoke<boolean>("path_exists", { path: effectiveAddonsPath() });
  });
}

function resetSettings() {
  return withLoading(async () => {
    const reset = await invoke<AppSettings>("reset_app_settings");
    state.settings = reset;
    applySettingsToState(reset);
    state.addonsPathExists = await invoke<boolean>("path_exists", { path: effectiveAddonsPath() });
  });
}

function settingField(label: string, id: string, value: string, browse = false) {
  return `
    <div class="field setting-item">
      <label for="${escapeAttr(id)}">${escapeHtml(label)}</label>
      <div class="${browse ? "field-with-action" : ""}">
        <input id="${escapeAttr(id)}" value="${escapeAttr(value)}" placeholder="Leave blank for default" ${disabledAttr()} />
        ${browse ? `<button class="secondary icon-button browse-button" data-browse-target="${escapeAttr(id)}" title="Browse for ${escapeAttr(label)}" ${disabledAttr()}>${icon("folder")} Browse</button>` : ""}
      </div>
    </div>
  `;
}

function settingCheckbox(label: string, id: string, value: boolean) {
  return `
    <label class="checkbox-line setting-check" for="${escapeAttr(id)}">
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

function icon(name: IconName) {
  const paths: Record<IconName, string> = {
    check: '<path d="M20 6 9 17l-5-5"/>',
    folder: '<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7l-2-2H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2Z"/><path d="M2 10h20"/>',
    rotate: '<path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 3v6h6"/>',
    target: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/><path d="M12 2v3"/><path d="M12 19v3"/><path d="M2 12h3"/><path d="M19 12h3"/>',
  };
  return `<svg class="button-icon" viewBox="0 0 24 24" aria-hidden="true">${paths[name]}</svg>`;
}

function plainEsoText(value: string) {
  return value.replace(/\|c[0-9a-fA-F]{6,8}/g, "").replace(/\|r/g, "");
}

function renderEsoText(value: string) {
  const tagPattern = /\|c([0-9a-fA-F]{6}|[0-9a-fA-F]{8})(.*?)\|r/g;
  let output = "";
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = tagPattern.exec(value)) !== null) {
    output += escapeHtml(plainEsoText(value.slice(cursor, match.index)));
    const color = match[1].length === 8 ? match[1].slice(2) : match[1];
    output += `<span class="eso-color" style="color:#${escapeAttr(color)}">${escapeHtml(plainEsoText(match[2]))}</span>`;
    cursor = match.index + match[0].length;
  }

  output += escapeHtml(plainEsoText(value.slice(cursor)));
  return output;
}

render();
loadStartup().then(() => {
  if (!state.needsInitialSetup) {
    loadInstalled();
  }
});
