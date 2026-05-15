import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { renderEsoMarkup, renderInlineEsoMarkup, stripEsoMarkup } from "./bbcode";
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
  BrowseRemoteAddonsResponse,
  CachedImageResponse,
  CheckAddonsResponse,
  HttpCacheStatsResponse,
  ImportExistingAddonsResponse,
  InstallRemoteAddonResponse,
  InstalledAddonsResponse,
  LocalAddon,
  MatchResult,
  PlannedAction,
  PlanRemoteInstallResponse,
  PlanUpdateAllResponse,
  PlanUpdatesResponse,
  RemoteCategory,
  RemoteAddonDetailsWithLocalStateResponse,
  RemoveInstalledAddonResponse,
  SingleUpdateApplyResponse,
  SingleUpdatePlanResponse,
  UpdateAllAction,
} from "./types";

type Tab = "installed" | "search" | "settings";
type DetailsTab = "info" | "changelog";
type InstalledFilter = "all" | "update" | "unknown" | "current";
type InstalledSort = "name" | "updated" | "downloads" | "status";
type SearchMode = "most_downloaded" | "recent";
type IconName = "check" | "external" | "folder" | "rotate" | "target";
type OperationKind =
  | "general"
  | "startup"
  | "installed"
  | "search"
  | "details"
  | "settings"
  | "cache"
  | "install-plan"
  | "install-apply"
  | "update-plan"
  | "update-apply"
  | "remove-apply"
  | "update-all-plan"
  | "update-all-apply";

interface AppState {
  tab: Tab;
  path: string;
  loading: boolean;
  operation: OperationKind | null;
  operationTarget: string | null;
  error: string | null;
  warning: string | null;
  installed: InstalledAddonsResponse | null;
  searchQuery: string;
  searchAppliedQuery: string;
  searchMode: SearchMode;
  searchCategoryId: string;
  searchLoaded: boolean;
  searchLoadAttempted: boolean;
  searchCategoryWarning: string | null;
  remoteCategories: RemoteCategory[];
  installedQuery: string;
  installedFilter: InstalledFilter;
  installedSort: InstalledSort;
  searchLimit: number;
  searchResults: AddonSummary[];
  selectedSummary: AddonSummary | null;
  selectedDetails: AddonDetails | null;
  detailsTab: DetailsTab;
  lightboxImageUrl: string | null;
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
  removeResult: RemoveInstalledAddonResponse | null;
  removeConfirmLocal: LocalAddon | null;
  removeSavedVariables: boolean;
  updateAllPlan: PlanUpdateAllResponse | null;
  updateAllResult: ApplyUpdateAllResponse | null;
  settings: AppSettings | null;
  addonsPathExists: boolean | null;
  needsInitialSetup: boolean;
  detectedAddonsPath: string | null;
  setupAddonsPath: string;
  setupImportPath: string | null;
  setupExistingAddonsCount: number;
  httpCacheStats: HttpCacheStatsResponse | null;
  httpCacheStatsLoaded: boolean;
  cachedImageUrls: Record<string, string>;
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
  operation: null,
  operationTarget: null,
  error: null,
  warning: null,
  installed: null,
  searchQuery: "",
  searchAppliedQuery: "",
  searchMode: "most_downloaded",
  searchCategoryId: "",
  searchLoaded: false,
  searchLoadAttempted: false,
  searchCategoryWarning: null,
  remoteCategories: [],
  installedQuery: "",
  installedFilter: "all",
  installedSort: "status",
  searchLimit: 25,
  searchResults: [],
  selectedSummary: null,
  selectedDetails: null,
  detailsTab: "info",
  lightboxImageUrl: null,
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
  removeResult: null,
  removeConfirmLocal: null,
  removeSavedVariables: false,
  updateAllPlan: null,
  updateAllResult: null,
  settings: null,
  addonsPathExists: null,
  needsInitialSetup: false,
  detectedAddonsPath: null,
  setupAddonsPath: "",
  setupImportPath: null,
  setupExistingAddonsCount: 0,
  httpCacheStats: null,
  httpCacheStatsLoaded: false,
  cachedImageUrls: {},
};

const appRoot = document.querySelector<HTMLDivElement>("#app");

if (!appRoot) {
  throw new Error("missing app root");
}

const app = appRoot;

window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (state.removeConfirmLocal && !guardedOperationRunning()) {
    event.preventDefault();
    cancelRemoveAddon();
    return;
  }
  if (state.lightboxImageUrl) {
    event.preventDefault();
    closeImageLightbox();
    return;
  }
  if (hasDetailsOpen() && !guardedOperationRunning()) {
    event.preventDefault();
    closeDetails();
  }
});

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
        ${renderCurrentTab()}
        ${renderDetailsModal()}
        ${renderRemoveConfirmationModal()}
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
          <button class="primary icon-button" id="save-initial-setup" ${disabledAttr()}>${loadingButtonContent(`${icon("check")} Continue`, "Loading...", "settings")}</button>
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
          <button class="primary icon-button" id="confirm-initial-import" ${disabledAttr()}>${loadingButtonContent(`${icon("check")} Import As Current`, "Loading...", "settings")}</button>
        </div>
      </section>
    </div>
  `;
}

function renderRemoveConfirmationModal() {
  const local = state.removeConfirmLocal;
  if (!local) return "";
  return `
    <div class="modal-backdrop remove-confirmation" role="presentation">
      <section class="modal-panel remove-confirmation-panel" role="dialog" aria-modal="true" aria-labelledby="remove-addon-title">
        <div class="modal-icon danger-icon">${icon("target")}</div>
        <div class="modal-content">
          <h2 id="remove-addon-title">Remove addon?</h2>
          <p>This will delete the addon folder from AddOns.</p>
          <p class="modal-path" title="${escapeAttr(local.folder_name)}">${escapeHtml(local.folder_name)}</p>
          <label class="checkbox-line remove-savedvariables-option">
            <input id="remove-savedvariables" type="checkbox" ${state.removeSavedVariables ? "checked" : ""} ${disabledAttr()} />
            <span>Also delete SavedVariables for this addon</span>
          </label>
          <p class="helper-text">SavedVariables store addon settings and character/account data. Leave this unchecked to keep your settings.</p>
        </div>
        <div class="modal-actions">
          <button class="secondary" id="cancel-remove-addon" ${disabledAttr()}>Cancel</button>
          <button class="danger" id="confirm-remove-addon" ${disabledAttr()}>${loadingButtonContent("Remove addon", "Removing...", "remove-apply")}</button>
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
  const updateAllButton = shouldShowUpdateAllButton()
    ? `<button class="primary" id="plan-update-all-installed" ${disabledAttr()}>${loadingButtonContent("Update All", "Preparing update...", "update-all-plan")}</button>`
    : "";
  return `
    ${pageHeader(
      "Installed Addons",
      "",
      `
        ${updateAllButton}
        <button class="secondary" id="refresh-installed" ${disabledAttr()}>${loadingButtonContent("Refresh", "Loading...", "installed")}</button>
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
    ${hasDetailsOpen() ? "" : renderSingleUpdatePlan()}
    ${hasDetailsOpen() ? "" : renderSingleUpdateResult()}
  `;
}

function renderInstalledList(view = installedView()) {
  if (isInstalledLoading()) return renderSkeletonCards(6);
  if (view.length === 0) {
    return emptyState("No addons to show", state.installed ? "Try another filter or refresh this AddOns directory." : "Refresh to scan your AddOns directory.");
  }
  return view.map(renderInstalledCard).join("");
}

function shouldShowUpdateAllButton() {
  if (isInstalledLoading()) return false;
  return (state.updatePlan?.actions ?? []).some(isActionableUpdateAction);
}

function isActionableUpdateAction(action: PlannedAction) {
  return action.action === "would-update" && action.update_confidence === "reliable-update";
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
  const hasQuery = state.searchAppliedQuery.trim().length > 0;
  const hasCategoryFilter = state.searchCategoryId.trim().length > 0;
  const showSearchSkeleton = isSearchLoading() || (!state.searchLoaded && !state.error);
  const resultTitle = hasQuery
    ? `Search results for "${state.searchAppliedQuery.trim()}"`
    : state.searchMode === "recent"
      ? "Recent addons"
      : "Most downloaded addons";
  return `
    ${pageHeader("Search", "Discover addons from remote metadata.", "")}
    <section class="control-panel search-controls">
      <div class="field search-mode-field">
        <span>Mode</span>
        <div class="chip-row search-mode-buttons" role="group" aria-label="Search mode">
          ${searchModeButton("most_downloaded", "Most Downloaded")}
          ${searchModeButton("recent", "Recent")}
        </div>
      </div>
      <label class="field">
        <span>Search term</span>
        <div class="field-with-action">
          <input id="search-query" value="${escapeAttr(state.searchQuery)}" placeholder="Addon name, author, or keyword" ${disabledAttr()} />
          <button class="primary" id="run-search" ${disabledAttr()}>${loadingButtonContent("Search", "Searching...", "search")}</button>
        </div>
      </label>
      <label class="field category-field">
        <span>Category</span>
        <select id="search-category" ${disabledAttr()} ${state.remoteCategories.length === 0 ? "disabled" : ""}>
          <option value="">All categories</option>
          ${state.remoteCategories.map((category) => `<option value="${escapeAttr(category.id)}" ${state.searchCategoryId === category.id ? "selected" : ""}>${escapeHtml(category.name)}</option>`).join("")}
        </select>
      </label>
      <label class="field limit-field">
        <span>Limit</span>
        <select id="search-limit" ${disabledAttr()}>
          ${[10, 25, 50, 100].map((value) => `<option value="${value}" ${state.searchLimit === value ? "selected" : ""}>${value}</option>`).join("")}
        </select>
      </label>
    </section>
    ${state.searchCategoryWarning ? `<div class="banner warning compact-banner">${escapeHtml(state.searchCategoryWarning)}</div>` : ""}
    ${state.searchLoaded || showSearchSkeleton ? `<p class="result-caption">${escapeHtml(resultTitle)}${hasCategoryFilter ? ` - ${escapeHtml(selectedCategoryName())}` : ""}</p>` : ""}
    <section class="addon-list">
      ${
        showSearchSkeleton
          ? renderSkeletonCards(6)
          : !state.searchLoaded
            ? emptyState("Remote addons unavailable", "Resolve the error above, then refresh Search.")
            : state.searchResults.length === 0
              ? emptyState("No matching addons", "No remote addons matched the current mode, category, and search filters.")
            : state.searchResults.map(renderSearchCard).join("")
      }
    </section>
  `;
}

function searchModeButton(mode: SearchMode, label: string) {
  return `<button class="chip search-mode-button ${state.searchMode === mode ? "active" : ""}" data-search-mode="${mode}" ${disabledAttr()}>${escapeHtml(label)}</button>`;
}

function selectedCategoryName() {
  return state.remoteCategories.find((category) => category.id === state.searchCategoryId)?.name ?? "Selected category";
}

function renderSearchCard(addon: AddonSummary) {
  const category = categoryMeta(addon.category_name, addon.category_id, false);
  return `
    <article class="addon-card clickable${addon.installed ? " is-installed" : ""}" data-addon-id="${escapeAttr(addon.uid ?? "")}">
      ${CategoryIcon(category)}
      <div class="addon-main">
        <div class="addon-title-row">
          <div>
            <h3>${renderEsoText(addon.name ?? "Unnamed addon")}</h3>
            <p>${escapeHtml(addon.author_name ? `by ${plainEsoText(addon.author_name)}` : "Author unknown")} &middot; ${escapeHtml(category.name)}</p>
          </div>
        </div>
        <div class="meta-grid">
          ${metaItem("Version", addon.version)}
          ${metaItem("Downloads", formatCount(addon.downloads))}
          ${metaItem("Updated", addon.updated_display)}
          ${metaItem("Category", category.name)}
        </div>
      </div>
      <div class="card-actions"></div>
      ${addon.installed ? installedCornerMarker() : ""}
    </article>
  `;
}

function installedCornerMarker() {
  return `
    <span class="installed-corner" title="Installed locally" role="img" aria-label="Installed locally">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M20 6 9 17l-5-5"></path>
      </svg>
    </span>
  `;
}

function renderDetailsModal() {
  const details = state.selectedDetails;
  const local = state.selectedLocal;
  const match = state.selectedMatch;
  const summary = state.selectedSummary;
  if (!details && !local && !summary) return "";
  if (isDetailsLoading() && !details) return renderDetailsSkeletonModal();
  const category = categoryMeta(
    details?.category_name ?? match?.remote?.category_name ?? summary?.category_name ?? null,
    details?.category_id ?? match?.remote?.category_id ?? summary?.category_id ?? null,
    local?.is_library ?? false,
  );
  const title = details?.name ?? local?.title ?? match?.remote?.name ?? summary?.name ?? local?.folder_name ?? "Addon Details";
  const author = details?.author_name ?? match?.remote?.author_name ?? summary?.author_name ?? local?.author ?? null;
  const installedVersion = local?.display_version ?? null;
  const remoteVersion = details?.version ?? match?.remote?.version ?? summary?.version ?? null;
  const downloads = details?.downloads ?? match?.remote?.downloads ?? summary?.downloads ?? null;
  const updated = details?.updated_display ?? match?.remote?.updated_display ?? summary?.updated_display ?? null;
  const statusNote = selectedDetailsStatusNote();
  const websiteUrl = selectedWebsiteUrl();
  const closeDisabled = guardedOperationRunning() ? "disabled" : "";
  return `
    <div class="details-modal-backdrop" id="close-details-backdrop"></div>
    <section class="addon-modal" role="dialog" aria-modal="true" aria-labelledby="addon-details-title">
      <header class="addon-modal-header">
        <div class="detail-identity">
          ${CategoryIcon(category, true)}
          <div class="detail-title-block">
            <h2 id="addon-details-title">${renderInlineEsoMarkup(title)}</h2>
            <p>${escapeHtml(author ? `by ${stripEsoMarkup(author)}` : "Author unknown")} &middot; ${escapeHtml(category.name)}</p>
            ${statusNote ? `<p class="detail-status-note">${escapeHtml(statusNote)}</p>` : ""}
          </div>
        </div>
        <div class="addon-modal-header-actions">
          <button class="secondary icon-button modal-website-button" id="open-website" title="Open in website" ${websiteUrl ? "" : "disabled"}>${icon("external")} Website</button>
          <button class="modal-close-button" id="close-details" aria-label="Close details" ${closeDisabled}>Close</button>
        </div>
      </header>
      <div class="addon-modal-scroll">
        <div class="meta-grid detail-meta">
          ${metaItem("Installed", installedVersion)}
          ${metaItem("Remote", remoteVersion)}
          ${metaItem("Downloads", formatCount(downloads))}
          ${metaItem("Updated", updated)}
        </div>
        ${renderDetailsActionPanels()}
        ${renderDetailsTabs()}
        <section class="details-tab-panel">
          ${state.detailsTab === "changelog" ? renderChangelogTab() : renderAddonInfoTab()}
        </section>
      </div>
      <footer class="addon-modal-footer">
        ${renderDetailsFooterActions()}
      </footer>
      ${renderImageLightbox()}
    </section>
  `;
}

function renderDetailsSkeletonModal() {
  const closeDisabled = guardedOperationRunning() ? "disabled" : "";
  return `
    <div class="details-modal-backdrop" id="close-details-backdrop"></div>
    <section class="addon-modal" role="dialog" aria-modal="true" aria-busy="true" aria-label="Loading addon details">
      <header class="addon-modal-header">
        <div class="detail-identity">
          ${skeletonIcon(true)}
          <div class="detail-title-block skeleton-stack">
            ${skeletonLine("skeleton-line-title")}
            ${skeletonLine("skeleton-line-subtitle")}
          </div>
        </div>
        <div class="addon-modal-header-actions">
          ${skeletonButton()}
          <button class="modal-close-button" id="close-details" aria-label="Close details" ${closeDisabled}>Close</button>
        </div>
      </header>
      <div class="addon-modal-scroll">
        ${renderSkeletonMetaGrid(4, "detail-meta")}
        <section class="screenshot-gallery skeleton-screenshot-gallery" aria-label="Loading screenshots">
          ${Array.from({ length: 3 }, () => `<span class="skeleton skeleton-screenshot" aria-hidden="true"></span>`).join("")}
        </section>
        <section class="prose-block skeleton-prose" aria-label="Loading description">
          ${skeletonLine("skeleton-line-wide")}
          ${skeletonLine("skeleton-line-full")}
          ${skeletonLine("skeleton-line-full")}
          ${skeletonLine("skeleton-line-medium")}
          ${skeletonLine("skeleton-line-long")}
        </section>
      </div>
      <footer class="addon-modal-footer">
        ${skeletonButton()}
        <button class="secondary" id="close-details-footer" ${closeDisabled}>Close</button>
      </footer>
    </section>
  `;
}

function renderDetailsActionPanels() {
  return `
    ${renderInstallPlan()}
    ${renderInstallResult()}
    ${renderSingleUpdatePlan()}
    ${renderSingleUpdateResult()}
    ${renderRemoveResult()}
  `;
}

function renderDetailsTabs() {
  return `
    <div class="details-tabs" role="tablist" aria-label="Addon details sections">
      <button class="details-tab ${state.detailsTab === "info" ? "active" : ""}" data-details-tab="info" role="tab" aria-selected="${state.detailsTab === "info"}">Addon Info</button>
      <button class="details-tab ${state.detailsTab === "changelog" ? "active" : ""}" data-details-tab="changelog" role="tab" aria-selected="${state.detailsTab === "changelog"}">Changelog</button>
    </div>
  `;
}

function renderAddonInfoTab() {
  const description =
    state.selectedDetails?.description ??
    state.selectedSummary?.summary ??
    state.selectedMatch?.remote?.summary ??
    state.selectedLocal?.description ??
    null;
  return `
    ${renderScreenshotGallery()}
    ${description ? `<section class="prose-block">${renderEsoMarkup(description)}</section>` : emptyState("No description", "No addon description is available.")}
  `;
}

function renderChangelogTab() {
  const changelog = state.selectedDetails?.changelog ?? null;
  if (!changelog?.trim()) {
    return emptyState("No changelog", "No changelog is available for this addon.");
  }
  return `<section class="prose-block">${renderEsoMarkup(changelog)}</section>`;
}

function renderScreenshotGallery() {
  const images = selectedImageUrls();
  if (images.length === 0) return "";
  const title = state.selectedDetails?.name ?? state.selectedSummary?.name ?? state.selectedLocal?.title ?? state.selectedLocal?.folder_name ?? "Addon screenshot";
  return `
    <section class="screenshot-gallery" aria-label="Addon screenshots">
      ${images
        .map(
          (url, index) => `
            <button class="screenshot-frame" type="button" data-lightbox-url="${escapeAttr(url)}" title="View larger screenshot">
              <img class="screenshot-image" src="${escapeAttr(displayImageUrl(url))}" alt="${escapeAttr(`${stripEsoMarkup(title)} screenshot ${index + 1}`)}" loading="lazy" />
            </button>
          `,
        )
        .join("")}
    </section>
  `;
}

function renderDetailsFooterActions() {
  if (hasResultSuccess()) {
    return `<button class="primary" id="close-details-footer" ${guardedOperationRunning() ? "disabled" : ""}>Close</button>`;
  }
  const removeAddon = state.selectedLocal
    ? `<button class="danger" id="remove-addon" ${disabledAttr()}>${loadingButtonContent("Remove addon", "Removing...", "remove-apply")}</button>`
    : "";
  return `
    ${removeAddon}
    ${renderInstallUpdateFooterAction()}
    <button class="secondary" id="close-details-footer" ${guardedOperationRunning() ? "disabled" : ""}>Close</button>
  `;
}

function renderInstallUpdateFooterAction() {
  const details = state.selectedDetails;
  const addonId = details?.uid ?? state.selectedSummary?.uid;
  const match = state.selectedMatch;

  if (!state.selectedLocal && addonId) {
    if (state.installPlan && !state.installResult) {
      if (isSafeNewInstallPlan(state.installPlan) || !hasInstallablePlanItems(state.installPlan)) return "";
      return `<button class="danger" id="confirm-install" ${disabledAttr()}>${loadingButtonContent("Confirm Install", "Installing...", "install-apply")}</button>`;
    }
    if (!state.installResult) {
      return `<button class="primary" id="plan-install" ${disabledAttr()}>${loadingButtonContent("Install", "Preparing install...", "install-plan")}</button>`;
    }
    return "";
  }

  if (!match) return "";
  const target = match.local.folder_name;
  const matchingPlan = state.singleUpdatePlan?.target.toLowerCase() === target.toLowerCase() ? state.singleUpdatePlan : null;
  if (matchingPlan?.should_install) {
    const label = matchingPlan.decision === "forced-reinstall" ? "Reinstall" : "Update";
    return `<button class="danger" id="confirm-update" ${disabledAttr()}>${loadingButtonContent(`Confirm ${label}`, "Updating...", "update-apply", target)}</button>`;
  }
  if (match.update_confidence === "reliable-update") {
    return `<button class="primary" data-plan-update-target="${escapeAttr(target)}" ${disabledAttr()}>${loadingButtonContent("Update", "Preparing update...", "update-plan", target)}</button>`;
  }
  if (state.forceUpdate && ["matched", "unknown-update", "local-newer"].includes(match.status)) {
    return `<button class="secondary" data-plan-update-target="${escapeAttr(target)}" ${disabledAttr()}>${loadingButtonContent("Reinstall", "Preparing update...", "update-plan", target)}</button>`;
  }
  return "";
}

function selectedDetailsStatusNote() {
  const local = state.selectedLocal;
  const match = state.selectedMatch;
  if (state.removeResult?.removed_addon) return "Addon removed";
  if (!local) {
    if (isOperation("install-apply")) return "Installing...";
    if (state.installPlan && !state.installResult) return "Install preview ready";
    if (state.installResult) return "Installed successfully";
    return "Not installed locally";
  }
  if (state.singleUpdatePlan?.reason) return state.singleUpdatePlan.reason;
  if (state.singleUpdateResult) return "Update completed";
  const status = installedStatus(match, local);
  const localName = plainEsoText(local.title?.trim() || local.folder_name);
  const version = local.display_version ? `, version ${local.display_version}` : "";
  const statusNote = installedStatusNote(status);
  return `Installed locally: ${localName} (${local.folder_name}${version})${statusNote ? ` - ${statusNote}` : ""}`;
}

function renderImageLightbox() {
  const url = state.lightboxImageUrl;
  if (!url) return "";
  return `
    <div class="image-lightbox" id="close-image-lightbox-backdrop" role="presentation">
      <div class="image-lightbox-panel" id="image-lightbox-panel" role="dialog" aria-modal="true" aria-label="Screenshot preview">
        <button class="image-lightbox-close" id="close-image-lightbox" aria-label="Close screenshot preview">Close</button>
        <img class="image-lightbox-image" src="${escapeAttr(displayImageUrl(url))}" alt="Larger addon screenshot" />
      </div>
    </div>
  `;
}

function renderSettings() {
  const settings = state.settings;
  const addonsMissing = Boolean(settings?.addons_dir_override) && state.addonsPathExists === false;
  return `
    ${pageHeader("Settings", "Desktop defaults for install and update actions.", `
      <div class="toolbar-actions">
        <button class="secondary icon-button" id="reset-settings" ${disabledAttr()}>${loadingButtonContent(`${icon("rotate")} Reset`, "Loading...", "settings")}</button>
        <button class="primary icon-button" id="save-settings" ${disabledAttr()}>${loadingButtonContent(`${icon("check")} Save`, "Loading...", "settings")}</button>
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
    ${renderHttpCacheSettings()}
  `;
}

function renderHttpCacheSettings() {
  const stats = state.httpCacheStats;
  return `
    <section class="panel settings-section">
      <div class="panel-heading">
        <div>
          <h3>HTTP cache</h3>
          <p>Remote API responses and addon images cached locally.</p>
        </div>
        <div class="toolbar-actions">
          <button class="secondary" id="refresh-http-cache-stats" ${disabledAttr()}>${loadingButtonContent("Refresh", "Loading...", "cache")}</button>
          <button class="danger" id="clear-http-cache" ${disabledAttr()}>${loadingButtonContent("Clear cache", "Clearing...", "cache")}</button>
        </div>
      </div>
      <div class="meta-grid">
        ${metaItem("Cache size", stats?.size_display ?? (state.httpCacheStatsLoaded ? "0 B" : "Loading..."))}
        ${metaItem("Entries", stats?.entry_count ?? (state.httpCacheStatsLoaded ? 0 : null))}
        ${metaItem("Cache path", stats?.cache_dir ?? null)}
      </div>
    </section>
  `;
}

function renderInstallPlan() {
  if (isOperation("install-plan")) return renderPlanSkeletonPanel("Install Preview");
  const plan = state.installPlan;
  if (!plan) return "";
  if (state.installResult && isSafeNewInstallPlan(plan)) return "";
  if (isOperation("install-apply") && isSafeNewInstallPlan(plan)) {
    return `
      <section class="panel">
        <div class="banner info">Validated package. Installing...</div>
        <div class="panel-heading">
          <div>
            <h3>Installing</h3>
            <p>All validated addon folders are new installs.</p>
          </div>
        </div>
        ${renderPlanItems(plan.plan.items)}
      </section>
    `;
  }
  const requiresReplacementReview = hasReplacementPlanItems(plan);
  const hasInstallableItems = hasInstallablePlanItems(plan);
  const bannerClass = requiresReplacementReview || !hasInstallableItems || hasSkippedPlanItems(plan) ? "warning" : "info";
  const bannerText = requiresReplacementReview
    ? "Review required: this install will replace existing addon folders after creating backups."
    : !hasInstallableItems
      ? "No valid addon folders were found. Nothing can be installed from this package."
      : hasSkippedPlanItems(plan)
        ? "Review required: this package includes skipped or invalid folders. Only valid addon folders can be installed."
        : "Validated package. Review these file changes before continuing.";
  return `
    <section class="panel">
      <div class="banner ${bannerClass}">${bannerText}</div>
      <div class="panel-heading">
        <div>
          <h3>Install Preview</h3>
          <p>Review these file changes before continuing.</p>
        </div>
      </div>
      ${renderPlanItems(plan.plan.items)}
    </section>
  `;
}

function renderInstallResult() {
  const result = state.installResult;
  if (!result) return "";
  return renderCompactInstallResult(result);
}

function renderUpdateAllPlan() {
  if (isOperation("update-all-plan")) return renderPlanSkeletonPanel("All Updates Preview");
  const plan = state.updateAllPlan;
  if (!plan) return "";
  return `
    <section class="panel">
      <div class="banner info">Dry run only. This preview did not download, extract, modify, or delete addon files.</div>
      <div class="panel-heading">
        <div>
          <h3>All Updates Preview</h3>
          <p>${plan.summary.planned_updates} update candidate${plan.summary.planned_updates === 1 ? "" : "s"} in ${escapeHtml(plan.addons_dir)}</p>
        </div>
        ${plan.summary.planned_updates > 0 ? `<button class="danger" id="apply-update-all" ${disabledAttr()}>${loadingButtonContent("Confirm Update All", "Updating...", "update-all-apply")}</button>` : ""}
      </div>
      <div class="addon-list">
        ${
          plan.actions.length === 0
            ? emptyState("No update candidates", "No update candidates were found in this preview.")
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
          ${metaItem("Action", action.action)}
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
        ${summaryItem("Previewed", result.summary.planned_updates)}
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
  if (isOperation("update-plan")) return renderPlanSkeletonPanel("Single Update Preview");
  const plan = state.singleUpdatePlan;
  if (!plan) return "";
  return `
    <section class="panel">
      <div class="panel-heading">
        <div>
          <h3>Single Update Preview</h3>
          <p>${escapeHtml(plan.reason ?? plan.decision)}</p>
        </div>
      </div>
      ${plan.plan ? renderPlanItems(plan.plan.items) : emptyState("No file changes previewed", "This addon is not eligible for update with the current options.")}
    </section>
  `;
}

function renderSingleUpdateResult() {
  const result = state.singleUpdateResult;
  if (!result) return "";
  return renderCompactUpdateResult(result);
}

type CompactResultKind = "success" | "warning";

interface CompactResultInput {
  kind: CompactResultKind;
  title: string;
  message: string;
  backupDir?: string | null;
  detailsTitle?: string;
  details?: string;
}

function renderCompactInstallResult(result: InstallRemoteAddonResponse) {
  if (!result.applied) {
    return renderCompactResultPanel({
      kind: "warning",
      title: "Install finished without file changes",
      message: "No addon folders were installed.",
      detailsTitle: "Details",
      details: renderInstallResultDetails(result),
    });
  }

  if (result.skipped > 0 || (result.installed_new > 0 && result.replaced > 0)) {
    return renderCompactResultPanel({
      kind: "warning",
      title: "Installed with warnings",
      message: "Some items were skipped.",
      backupDir: result.backup_dir,
      detailsTitle: "Details",
      details: renderInstallResultDetails(result),
    });
  }

  if (result.replaced > 0) {
    return renderCompactResultPanel({
      kind: "success",
      title: "Replaced successfully",
      message: result.backup_dir ? "A backup was created." : "The addon was replaced.",
      backupDir: result.backup_dir,
    });
  }

  return renderCompactResultPanel({
    kind: "success",
    title: "Installed successfully",
    message: "The addon is ready to use in ESO.",
  });
}

function renderCompactUpdateResult(result: SingleUpdateApplyResponse) {
  if (!result.applied) {
    return renderCompactResultPanel({
      kind: "warning",
      title: "Update finished without file changes",
      message: "No addon folders were updated.",
      detailsTitle: "Details",
      details: renderUpdateResultDetails(result),
    });
  }

  if (result.skipped > 0 || (result.installed_new > 0 && result.replaced > 0)) {
    return renderCompactResultPanel({
      kind: "warning",
      title: "Updated with warnings",
      message: "Some items were skipped.",
      backupDir: result.backup_dir,
      detailsTitle: "Details",
      details: renderUpdateResultDetails(result),
    });
  }

  return renderCompactResultPanel({
    kind: "success",
    title: "Updated successfully",
    message: result.backup_dir ? "A backup was created." : "The addon is ready to use in ESO.",
    backupDir: result.backup_dir,
  });
}

function renderRemoveResult() {
  const result = state.removeResult;
  if (!result) return "";

  return renderCompactResultPanel({
    kind: result.removed_addon ? "success" : "warning",
    title: result.removed_addon ? "Addon removed" : "Remove finished without file changes",
    message: result.message,
    detailsTitle: result.saved_variables_deleted_count > 0 ? "Deleted SavedVariables" : undefined,
    details:
      result.saved_variables_deleted_count > 0
        ? renderSavedVariablesDeletedFiles(result.saved_variables_deleted_files)
        : undefined,
  });
}

function renderSavedVariablesDeletedFiles(files: string[]) {
  return `
    <div class="mini-list">
      ${files
        .map(
          (file) => `
            <div class="mini-row">
              <strong>${escapeHtml(file)}</strong>
              <span>deleted</span>
              <span>SavedVariables</span>
            </div>
          `,
        )
        .join("")}
    </div>
  `;
}

function renderCompactResultPanel(result: CompactResultInput) {
  return `
    <section class="panel result-panel ${result.kind === "success" ? "result-success" : "result-warning"}">
      <div class="result-state">
        <span class="result-icon" aria-hidden="true">${result.kind === "success" ? resultCheckIcon() : resultWarningIcon()}</span>
        <div>
          <h3>${escapeHtml(result.title)}</h3>
          <p>${escapeHtml(result.message)}</p>
        </div>
      </div>
      ${result.backupDir ? renderBackupDetails(result.backupDir) : ""}
      ${result.details ? renderCollapsedDetails(result.detailsTitle ?? "Details", result.details) : ""}
    </section>
  `;
}

function renderBackupDetails(backupDir: string) {
  return `
    <details class="result-details">
      <summary>Details</summary>
      <p class="technical-path" title="${escapeAttr(backupDir)}">${escapeHtml(backupDir)}</p>
    </details>
  `;
}

function renderCollapsedDetails(title: string, content: string) {
  return `
    <details class="result-details">
      <summary>${escapeHtml(title)}</summary>
      ${content}
    </details>
  `;
}

function renderInstallResultDetails(result: InstallRemoteAddonResponse) {
  return `
    <div class="summary compact-summary">
      ${summaryItem("Installed", result.installed_new)}
      ${summaryItem("Replaced", result.replaced)}
      ${summaryItem("Skipped", result.skipped)}
      ${summaryItem("Applied", result.applied ? 1 : 0)}
    </div>
    <p class="technical-path" title="${escapeAttr(result.addons_dir)}">Target AddOns path: ${escapeHtml(result.addons_dir)}</p>
    ${renderInstalledItems(result.items)}
  `;
}

function renderUpdateResultDetails(result: SingleUpdateApplyResponse) {
  return `
    <div class="summary compact-summary">
      ${summaryItem("Installed", result.installed_new)}
      ${summaryItem("Replaced", result.replaced)}
      ${summaryItem("Skipped", result.skipped)}
      ${summaryItem("Applied", result.applied ? 1 : 0)}
    </div>
    <p class="technical-path" title="${escapeAttr(result.addons_dir)}">Target AddOns path: ${escapeHtml(result.addons_dir)}</p>
    ${renderInstalledItems(result.items)}
  `;
}

function resultCheckIcon() {
  return `<svg viewBox="0 0 24 24"><path d="M20 6 9 17l-5-5"></path></svg>`;
}

function resultWarningIcon() {
  return `<svg viewBox="0 0 24 24"><path d="M12 9v4"></path><path d="M12 17h.01"></path><path d="M10.3 4.6 2.8 18a2 2 0 0 0 1.7 3h15a2 2 0 0 0 1.7-3L13.7 4.6a2 2 0 0 0-3.4 0Z"></path></svg>`;
}

function renderPlanItems(items: { source_folder: string | null; target_folder: string | null; action: string; title: string | null; version: string | null }[]) {
  if (items.length === 0) return emptyState("No preview items", "No addon folders were found in this preview.");
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

function isSafeNewInstallPlan(plan: PlanRemoteInstallResponse) {
  return plan.plan.items.length > 0 && plan.plan.items.every((item) => item.action === "would-install-new");
}

function hasReplacementPlanItems(plan: PlanRemoteInstallResponse) {
  return plan.plan.items.some((item) => item.action === "would-replace-existing");
}

function hasInstallablePlanItems(plan: PlanRemoteInstallResponse) {
  return plan.plan.items.some((item) => item.action === "would-install-new" || item.action === "would-replace-existing");
}

function hasSkippedPlanItems(plan: PlanRemoteInstallResponse) {
  return plan.plan.items.some((item) => item.action !== "would-install-new" && item.action !== "would-replace-existing");
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
      if (state.tab === "search") {
        ensureSearchLoaded();
      }
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
  document.querySelector<HTMLButtonElement>("#refresh-installed")?.addEventListener("click", () => loadInstalled(true));
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
    const value = (event.currentTarget as HTMLInputElement).value;
    state.searchQuery = value;
    if (!value.trim() && state.searchAppliedQuery) {
      state.searchAppliedQuery = "";
      void loadSearchResults();
    }
  });
  document.querySelector<HTMLInputElement>("#search-query")?.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      runSearch();
    }
  });
  document.querySelector<HTMLSelectElement>("#search-limit")?.addEventListener("change", (event) => {
    state.searchLimit = Number((event.currentTarget as HTMLSelectElement).value);
    void loadSearchResults();
  });
  document.querySelector<HTMLSelectElement>("#search-category")?.addEventListener("change", (event) => {
    state.searchCategoryId = (event.currentTarget as HTMLSelectElement).value;
    void loadSearchResults();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-search-mode]").forEach((button) => {
    button.addEventListener("click", () => {
      state.searchMode = button.dataset.searchMode as SearchMode;
      void loadSearchResults();
    });
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
  document.querySelector<HTMLButtonElement>("#close-details")?.addEventListener("click", requestCloseDetails);
  document.querySelector<HTMLButtonElement>("#close-details-footer")?.addEventListener("click", requestCloseDetails);
  document.querySelector<HTMLDivElement>("#close-details-backdrop")?.addEventListener("click", requestCloseDetails);
  document.querySelector<HTMLButtonElement>("#open-website")?.addEventListener("click", openSelectedWebsite);
  document.querySelectorAll<HTMLAnchorElement>(".prose-block a[href]").forEach((link) => {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      void openExternalUrl(link.href);
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-details-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.detailsTab = button.dataset.detailsTab as DetailsTab;
      state.lightboxImageUrl = null;
      render();
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-lightbox-url]").forEach((button) => {
    button.addEventListener("click", () => openImageLightbox(button.dataset.lightboxUrl ?? ""));
  });
  document.querySelector<HTMLButtonElement>("#close-image-lightbox")?.addEventListener("click", closeImageLightbox);
  document.querySelector<HTMLDivElement>("#close-image-lightbox-backdrop")?.addEventListener("click", closeImageLightbox);
  document.querySelector<HTMLDivElement>("#image-lightbox-panel")?.addEventListener("click", (event) => event.stopPropagation());
  document.querySelectorAll<HTMLImageElement>(".screenshot-image, .bbcode-inline-image").forEach((image) => {
    image.addEventListener("error", () => image.closest(".screenshot-frame, .bbcode-image-frame")?.classList.add("image-failed"));
  });
  document.querySelector<HTMLImageElement>(".image-lightbox-image")?.addEventListener("error", closeImageLightbox);
  document.querySelector<HTMLButtonElement>("#plan-install")?.addEventListener("click", planInstall);
  document.querySelector<HTMLButtonElement>("#confirm-install")?.addEventListener("click", confirmInstall);
  document.querySelector<HTMLButtonElement>("#remove-addon")?.addEventListener("click", removeAddon);
  document.querySelector<HTMLButtonElement>("#cancel-remove-addon")?.addEventListener("click", cancelRemoveAddon);
  document.querySelector<HTMLButtonElement>("#confirm-remove-addon")?.addEventListener("click", confirmRemoveAddon);
  document.querySelector<HTMLInputElement>("#remove-savedvariables")?.addEventListener("change", (event) => {
    state.removeSavedVariables = (event.currentTarget as HTMLInputElement).checked;
  });
  document.querySelector<HTMLButtonElement>("#refresh-installed-after-install")?.addEventListener("click", () => loadInstalled(true));
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
  document.querySelector<HTMLButtonElement>("#refresh-http-cache-stats")?.addEventListener("click", loadHttpCacheStats);
  document.querySelector<HTMLButtonElement>("#clear-http-cache")?.addEventListener("click", clearHttpCache);
  document.querySelector<HTMLInputElement>("#settings-addons-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-backup-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-download-dir")?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-keep-downloads")?.addEventListener("change", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-include-unknown")?.addEventListener("change", syncSettingsDraft);
  document.querySelectorAll<HTMLButtonElement>("[data-browse-target]").forEach((button) => {
    button.addEventListener("click", () => browseSettingsFolder(button.dataset.browseTarget ?? ""));
  });
  if (state.tab === "search") {
    ensureSearchLoaded();
  }
  if (state.tab === "settings") {
    ensureHttpCacheStatsLoaded();
  }
}

async function withLoading(task: () => Promise<void>, operation: OperationKind = "general", operationTarget: string | null = null) {
  state.loading = true;
  state.operation = operation;
  state.operationTarget = operationTarget;
  state.error = null;
  render();
  try {
    await task();
  } catch (error) {
    state.error = error instanceof Error ? error.message : String(error);
  } finally {
    state.loading = false;
    state.operation = null;
    state.operationTarget = null;
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

function loadInstalled(refresh = true) {
  return withLoading(async () => {
    if (!state.settings) {
      state.settings = await invoke<AppSettings>("get_app_settings");
      applySettingsToState(state.settings);
    }
    await refreshInstalledData(refresh);
  }, "installed");
}

async function refreshInstalledData(refresh = false) {
  state.addonsPathExists = await invoke<boolean>("path_exists", { path: effectiveAddonsPath() });
  state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: effectiveAddonsPath() });
  state.path = state.installed.addons_dir;
  try {
    const updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
      path: effectiveAddonsPath(),
      includeUnknown: updateIncludeUnknownDefault(),
      refresh,
    });
    state.updatePlan = updatePlan;
    state.updates = updatesFromPlan(updatePlan);
    state.warning = updatePlan.cache_warning;
  } catch (error) {
    state.updatePlan = null;
    state.updates = null;
    state.warning = `Remote metadata could not be loaded. Showing local addons only. ${error instanceof Error ? error.message : String(error)}`;
  }
}

function runSearch() {
  state.searchAppliedQuery = state.searchQuery.trim();
  return loadSearchResults(true);
}

function ensureSearchLoaded() {
  if (state.searchLoaded || state.searchLoadAttempted || state.operation === "search") return;
  void loadSearchResults();
}

function loadSearchResults(refresh = false) {
  state.searchLoadAttempted = true;
  return withLoading(async () => {
    const response = await invoke<BrowseRemoteAddonsResponse>("browse_remote_addons", {
      mode: state.searchMode,
      categoryId: state.searchCategoryId || null,
      query: state.searchAppliedQuery || null,
      limit: state.searchLimit,
      path: effectiveAddonsPath(),
      refresh,
    });
    state.searchMode = response.mode === "recent" ? "recent" : "most_downloaded";
    state.searchAppliedQuery = response.query;
    state.searchCategoryId = response.category_id ?? "";
    state.searchLimit = response.limit;
    state.searchResults = response.results;
    state.remoteCategories = response.categories;
    state.searchCategoryWarning = [response.cache_warning, response.category_warning, response.local_warning].filter(Boolean).join(" ");
    state.searchLoaded = true;
  }, "search");
}

function loadDetails(addonId: string) {
  if (!addonId) return;
  state.selectedSummary = state.searchResults.find((addon) => addon.uid === addonId) ?? null;
  state.selectedDetails = null;
  state.detailsTab = "info";
  state.lightboxImageUrl = null;
  state.cachedImageUrls = {};
  state.selectedLocal = state.selectedSummary?.installed_local ?? null;
  state.selectedMatch = state.selectedSummary?.installed_match ?? null;
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  return withLoading(async () => {
    const response = await invoke<RemoteAddonDetailsWithLocalStateResponse>("get_remote_addon_details_with_local_state", {
      addonId,
      path: effectiveAddonsPath(),
    });
    if (state.selectedSummary?.uid === addonId) {
      state.selectedDetails = response.details;
      state.selectedLocal = response.local;
      state.selectedMatch = response.match_result;
      state.warning = [response.cache_warning, response.local_warning].filter(Boolean).join(" ") || null;
      void cacheSelectedImages();
    }
  }, "details");
}

function openInstalledDetails(folderName: string) {
  const addon = state.installed?.addons.find((item) => item.folder_name === folderName) ?? null;
  const match = state.updates?.matches.find((item) => item.local.folder_name === folderName) ?? null;
  if (!addon) return;
  state.selectedLocal = addon;
  state.selectedMatch = match;
  state.selectedSummary = null;
  state.selectedDetails = null;
  state.detailsTab = "info";
  state.lightboxImageUrl = null;
  state.cachedImageUrls = {};
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  const uid = match?.remote?.uid;
  if (uid) {
    return withLoading(async () => {
      const details = await invoke<AddonDetails>("get_remote_addon_details", { addonId: uid });
      if (state.selectedLocal?.folder_name === folderName) {
        state.selectedDetails = details;
        void cacheSelectedImages();
      }
    }, "details");
  } else {
    render();
  }
}

function requestCloseDetails() {
  if (guardedOperationRunning()) return;
  closeDetails();
}

function closeDetails() {
  state.selectedDetails = null;
  state.selectedSummary = null;
  state.selectedLocal = null;
  state.selectedMatch = null;
  state.detailsTab = "info";
  state.lightboxImageUrl = null;
  state.cachedImageUrls = {};
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  state.removeConfirmLocal = null;
  state.removeSavedVariables = false;
  render();
}

function openImageLightbox(url: string) {
  const safeUrl = selectedImageUrls().find((imageUrl) => imageUrl === url);
  if (!safeUrl) return;
  state.lightboxImageUrl = safeUrl;
  render();
}

function closeImageLightbox() {
  if (!state.lightboxImageUrl) return;
  state.lightboxImageUrl = null;
  render();
}

function planInstall() {
  const addonId = state.selectedDetails?.uid ?? state.selectedSummary?.uid;
  if (!addonId) return;
  state.installPlan = null;
  state.installResult = null;
  state.removeResult = null;
  return withLoading(async () => {
    state.installPlan = await invoke<PlanRemoteInstallResponse>("plan_remote_install", {
      addonId,
      path: effectiveAddonsPath(),
    });
    state.path = state.installPlan.addons_dir;
    state.installResult = null;
    state.removeResult = null;

    if (!hasInstallablePlanItems(state.installPlan)) {
      throw new Error("No valid addon folders were found in this package. Nothing was installed.");
    }

    if (isSafeNewInstallPlan(state.installPlan)) {
      state.operation = "install-apply";
      render();
      try {
        state.installResult = await invoke<InstallRemoteAddonResponse>("install_remote_addon_new_only", {
          addonId,
          path: effectiveAddonsPath(),
          backupDir: state.settings?.backup_dir_override || null,
          keepDownload: state.settings?.keep_downloads_default ?? false,
          downloadDir: state.settings?.download_dir || null,
        });
      } catch (error) {
        state.installPlan = null;
        throw error;
      }
      state.path = state.installResult.addons_dir;
      await refreshInstalledData(true);
      syncInstalledStateAfterInstall(state.installResult);
    }
  }, "install-plan");
}

function confirmInstall() {
  const addonId = state.selectedDetails?.uid ?? state.installPlan?.remote.uid ?? state.selectedSummary?.uid;
  const plan = state.installPlan;
  if (!addonId || !plan) return;
  if (!hasInstallablePlanItems(plan)) {
    state.error = "No valid addon folders were found in this package. Nothing was installed.";
    render();
    return;
  }
  const backupText = plan.plan.items.some((item) => item.action === "would-replace-existing")
    ? "Existing addon folders may be backed up and replaced."
    : "No existing addon folder replacement is currently expected.";
  const confirmed = window.confirm(
    `Install ${plan.remote.name ?? addonId}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n\nThe app will fetch fresh metadata, download and verify the ZIP, validate it, build a fresh preview, and back up replacements before applying.`,
  );
  if (!confirmed) return;
  state.removeResult = null;
  return withLoading(async () => {
    state.installResult = await invoke<InstallRemoteAddonResponse>("install_remote_addon", {
      addonId,
      path: effectiveAddonsPath(),
      backupDir: state.settings?.backup_dir_override || null,
      keepDownload: state.settings?.keep_downloads_default ?? false,
      downloadDir: state.settings?.download_dir || null,
    });
    state.path = state.installResult.addons_dir;
    await refreshInstalledData(true);
    syncInstalledStateAfterInstall(state.installResult);
  }, "install-apply");
}

function planSingleUpdate(target: string) {
  if (!target) return;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  state.updateAllPlan = null;
  state.updateAllResult = null;
  return withLoading(async () => {
    state.singleUpdatePlan = await invoke<SingleUpdatePlanResponse>("plan_single_update", {
      target,
      path: effectiveAddonsPath(),
      force: state.forceUpdate,
    });
    state.singleUpdateResult = null;
    state.removeResult = null;
    state.updateAllPlan = null;
    state.updateAllResult = null;
    state.path = state.singleUpdatePlan.addons_dir;
  }, "update-plan", target);
}

function confirmUpdate() {
  const plan = state.singleUpdatePlan;
  if (!plan || !plan.should_install || !plan.plan) return;
  const backupText = plan.plan.items.some((item) => item.action === "would-replace-existing")
    ? "Existing addon folders may be backed up and replaced."
    : "No existing addon folder replacement is currently expected.";
  const confirmed = window.confirm(
    `Update ${plan.local.folder_name}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n\nThe app will match the addon again, fetch fresh metadata, download and verify the ZIP, validate it, build a fresh preview, and back up replacements before applying.`,
  );
  if (!confirmed) return;
  state.removeResult = null;
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
    await refreshUpdatePlan(true);
  }, "update-apply", plan.target);
}

function removeAddon() {
  const local = state.selectedLocal;
  if (!local) return;
  state.removeConfirmLocal = local;
  state.removeSavedVariables = false;
  render();
}

function cancelRemoveAddon() {
  if (guardedOperationRunning()) return;
  state.removeConfirmLocal = null;
  state.removeSavedVariables = false;
  render();
}

function confirmRemoveAddon() {
  const local = state.removeConfirmLocal;
  if (!local) return;
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  return withLoading(async () => {
    state.removeResult = await invoke<RemoveInstalledAddonResponse>("remove_installed_addon", {
      folderName: local.folder_name,
      path: effectiveAddonsPath(),
      removeSavedVariables: state.removeSavedVariables,
    });
    state.removeConfirmLocal = null;
    state.removeSavedVariables = false;
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: effectiveAddonsPath() });
    state.path = state.installed.addons_dir;
    syncInstalledStateAfterRemove(state.removeResult);
  }, "remove-apply");
}

function syncInstalledStateAfterInstall(result: InstallRemoteAddonResponse) {
  const installedLocal = installedLocalFromResult(result);
  const remoteUid = result.remote.uid ?? state.selectedDetails?.uid ?? state.selectedSummary?.uid ?? null;
  if (!installedLocal || !remoteUid) return;

  state.selectedLocal = installedLocal;
  state.searchResults = state.searchResults.map((addon) =>
    addon.uid === remoteUid ? { ...addon, installed: true, installed_local: installedLocal } : addon,
  );

  if (state.selectedSummary?.uid === remoteUid) {
    state.selectedSummary = {
      ...state.selectedSummary,
      installed: true,
      installed_local: installedLocal,
    };
  }
}

function syncInstalledStateAfterRemove(result: RemoveInstalledAddonResponse) {
  const folderName = result.addon_folder.toLowerCase();
  state.searchResults = state.searchResults.map((addon) =>
    addon.installed_local?.folder_name.toLowerCase() === folderName
      ? { ...addon, installed: false, installed_local: null, installed_match: null }
      : addon,
  );
  if (state.selectedSummary?.installed_local?.folder_name.toLowerCase() === folderName) {
    state.selectedSummary = {
      ...state.selectedSummary,
      installed: false,
      installed_local: null,
      installed_match: null,
    };
  }
  state.updates = state.updates
    ? {
        ...state.updates,
        matches: state.updates.matches.filter((match) => match.local.folder_name.toLowerCase() !== folderName),
      }
    : null;
  state.updatePlan = state.updatePlan
    ? {
        ...state.updatePlan,
        matches: state.updatePlan.matches.filter((match) => match.local.folder_name.toLowerCase() !== folderName),
        actions: state.updatePlan.actions.filter((action) => action.local_folder.toLowerCase() !== folderName),
      }
    : null;
}

function installedLocalFromResult(result: InstallRemoteAddonResponse) {
  const folderName = result.items
    .filter((item) => item.action === "installed-new" || item.action === "replaced-existing")
    .map((item) => folderNameFromPath(item.target_folder))
    .find(Boolean);

  if (!folderName) return null;
  return state.installed?.addons.find((addon) => addon.folder_name.toLowerCase() === folderName.toLowerCase()) ?? null;
}

function folderNameFromPath(value: string | null) {
  if (!value) return null;
  const parts = value.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? null;
}

function planUpdateAll() {
  state.updateAllPlan = null;
  state.updateAllResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
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
    state.removeResult = null;
  }, "update-all-plan");
}

function applyUpdateAll() {
  const plan = state.updateAllPlan;
  if (!plan || plan.summary.planned_updates === 0) return;
  const confirmed = window.confirm(
    `Update ${plan.summary.planned_updates} addon${plan.summary.planned_updates === 1 ? "" : "s"}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\nThe app will process updates sequentially, fetch fresh metadata for each addon, download and verify each ZIP, validate each package, and back up replacements before applying. It will stop on the first error.`,
  );
  if (!confirmed) return;
  state.removeResult = null;
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
    await refreshUpdatePlan(true);
  }, "update-all-apply");
}

function loadUpdates() {
  return withLoading(async () => {
    await refreshUpdatePlan(true);
    state.singleUpdatePlan = null;
    state.singleUpdateResult = null;
    state.updateAllPlan = null;
    state.updateAllResult = null;
  }, "update-plan");
}

async function refreshUpdatePlan(refresh = false) {
  const updatePlan = await invoke<PlanUpdatesResponse>("plan_updates", {
    path: effectiveAddonsPath(),
    includeUnknown: updateIncludeUnknownDefault(),
    refresh,
  });
  state.updatePlan = updatePlan;
  state.updates = updatesFromPlan(updatePlan);
  state.path = updatePlan.addons_dir;
  state.warning = updatePlan.cache_warning;
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
    return `<button class="primary small" data-plan-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>${loadingButtonContent("Update", "Preparing update...", "update-plan", match.local.folder_name)}</button>`;
  }
  if (state.forceUpdate && ["matched", "unknown-update", "local-newer"].includes(match.status)) {
    return `<button class="secondary small" data-plan-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>${loadingButtonContent("Reinstall", "Preparing update...", "update-plan", match.local.folder_name)}</button>`;
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

function renderSkeletonCards(count: number) {
  return Array.from({ length: count }, (_, index) => renderSkeletonAddonCard(index)).join("");
}

function renderSkeletonAddonCard(index = 0) {
  const titleClass = index % 3 === 1 ? "skeleton-line-long" : "skeleton-line-title";
  const subtitleClass = index % 2 === 0 ? "skeleton-line-medium" : "skeleton-line-short";
  return `
    <article class="addon-card skeleton-card" aria-hidden="true">
      ${skeletonIcon()}
      <div class="addon-main">
        <div class="addon-title-row">
          <div class="skeleton-stack skeleton-title-stack">
            ${skeletonLine(titleClass)}
            ${skeletonLine(subtitleClass)}
          </div>
          ${skeletonLine("skeleton-chip")}
        </div>
        ${renderSkeletonMetaGrid(4)}
      </div>
      <div class="card-actions">${skeletonButton()}</div>
    </article>
  `;
}

function renderSkeletonMetaGrid(count: number, extraClass = "") {
  return `
    <div class="meta-grid skeleton-meta-grid ${escapeAttr(extraClass)}">
      ${Array.from({ length: count }, (_, index) => renderSkeletonMetaBlock(index)).join("")}
    </div>
  `;
}

function renderSkeletonMetaBlock(index: number) {
  const valueClass = index % 2 === 0 ? "skeleton-line-short" : "skeleton-line-medium";
  return `
    <div class="meta-item skeleton-meta-block">
      ${skeletonLine("skeleton-line-label")}
      ${skeletonLine(valueClass)}
    </div>
  `;
}

function renderPlanSkeletonPanel(label: string) {
  return `
    <section class="panel skeleton-plan-panel" aria-busy="true" aria-label="${escapeAttr(label)} loading">
      <div class="panel-heading">
        <div class="skeleton-stack skeleton-panel-heading">
          ${skeletonLine("skeleton-line-heading")}
          ${skeletonLine("skeleton-line-medium")}
        </div>
        ${skeletonButton()}
      </div>
      <div class="mini-list">
        ${Array.from({ length: 4 }, () => renderSkeletonMiniRow()).join("")}
      </div>
    </section>
  `;
}

function renderSkeletonMiniRow() {
  return `
    <div class="mini-row skeleton-mini-row" aria-hidden="true">
      ${skeletonLine("skeleton-line-long")}
      ${skeletonLine("skeleton-line-short")}
      ${skeletonLine("skeleton-line-medium")}
    </div>
  `;
}

function skeletonLine(className = "") {
  return `<span class="skeleton skeleton-line ${escapeAttr(className)}" aria-hidden="true"></span>`;
}

function skeletonIcon(large = false) {
  return `<span class="skeleton skeleton-icon ${large ? "large" : ""}" aria-hidden="true"></span>`;
}

function skeletonButton() {
  return `<span class="skeleton skeleton-button" aria-hidden="true"></span>`;
}

function loadingButtonContent(defaultContent: string, loadingLabel: string, operation: OperationKind, target?: string) {
  if (!isOperation(operation, target)) return defaultContent;
  return `<span class="button-spinner" aria-hidden="true"></span>${escapeHtml(loadingLabel)}`;
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

function isOperation(operation: OperationKind, target?: string) {
  if (!state.loading || state.operation !== operation) return false;
  if (target === undefined) return true;
  return (state.operationTarget ?? "").toLowerCase() === target.toLowerCase();
}

function isInstalledLoading() {
  return state.loading && ["startup", "installed"].includes(state.operation ?? "");
}

function isSearchLoading() {
  return isOperation("search");
}

function isDetailsLoading() {
  return isOperation("details");
}

function hasResultSuccess() {
  return Boolean(
    (state.installResult?.applied || state.singleUpdateResult?.applied || state.removeResult?.removed_addon) &&
      !guardedOperationRunning(),
  );
}

function hasDetailsOpen() {
  return Boolean(state.selectedDetails || state.selectedLocal || state.selectedSummary);
}

function guardedOperationRunning() {
  return Boolean(
    state.operation &&
      ["install-plan", "install-apply", "update-plan", "update-apply", "remove-apply", "update-all-plan", "update-all-apply"].includes(
        state.operation,
      ),
  );
}

function selectedWebsiteUrl() {
  const url = state.selectedDetails?.file_info_url ?? state.selectedMatch?.remote?.file_info_url ?? state.selectedSummary?.file_info_url ?? null;
  return url && isSafeHttpUrl(url) ? url : null;
}

function selectedImageUrls() {
  const urls = [
    ...(state.selectedDetails?.image_urls ?? []),
    ...(state.selectedMatch?.remote?.image_urls ?? []),
    ...(state.selectedSummary?.image_urls ?? []),
    ...(state.selectedDetails?.thumbnail_urls ?? []),
    ...(state.selectedMatch?.remote?.thumbnail_urls ?? []),
    ...(state.selectedSummary?.thumbnail_urls ?? []),
  ];
  return uniqueSafeUrls(urls).slice(0, 8);
}

function displayImageUrl(url: string) {
  return state.cachedImageUrls[url] ?? url;
}

async function cacheSelectedImages() {
  const urls = selectedImageUrls();
  if (urls.length === 0) return;
  const selectedKey =
    state.selectedDetails?.uid ?? state.selectedSummary?.uid ?? state.selectedLocal?.folder_name ?? "";

  const responses = await Promise.allSettled(
    urls.map((url) => invoke<CachedImageResponse>("cache_remote_image", { url })),
  );
  let changed = false;
  const warnings: string[] = [];

  responses.forEach((response) => {
    if (response.status !== "fulfilled") return;
    state.cachedImageUrls[response.value.url] = response.value.data_url;
    changed = true;
    if (response.value.cache_warning) warnings.push(response.value.cache_warning);
  });

  const stillSelected =
    selectedKey === (state.selectedDetails?.uid ?? state.selectedSummary?.uid ?? state.selectedLocal?.folder_name ?? "");
  if (!changed || !stillSelected) return;
  const cacheWarning = Array.from(new Set(warnings)).join(" ") || null;
  if (cacheWarning) state.warning = cacheWarning;
  render();
}

function uniqueSafeUrls(urls: string[]) {
  const seen = new Set<string>();
  const output: string[] = [];
  for (const url of urls) {
    const trimmed = url.trim();
    if (!isSafeHttpUrl(trimmed) || seen.has(trimmed)) continue;
    seen.add(trimmed);
    output.push(trimmed);
  }
  return output;
}

function isSafeHttpUrl(value: string) {
  try {
    const url = new URL(value);
    return (url.protocol === "http:" || url.protocol === "https:") && Boolean(url.hostname);
  } catch {
    return false;
  }
}

async function openSelectedWebsite() {
  const url = selectedWebsiteUrl();
  if (!url) return;
  await openExternalUrl(url);
}

async function openExternalUrl(url: string) {
  if (!isSafeHttpUrl(url)) return;
  try {
    await invoke("open_external_url", { url });
  } catch (error) {
    state.error = `Could not open website. ${error instanceof Error ? error.message : String(error)}`;
    render();
  }
}

function updatesFromPlan(plan: PlanUpdatesResponse): CheckAddonsResponse {
  return {
    addons_dir: plan.addons_dir,
    remote_addons_loaded: plan.remote_addons_loaded,
    matches: plan.matches,
    cache_warning: plan.cache_warning,
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
  }, "startup");
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
  }, "settings");
}

function confirmInitialImport() {
  const selectedPath = state.setupImportPath;
  if (!selectedPath) return;
  return withLoading(async () => {
    await finishInitialSetup(selectedPath, true);
  }, "settings");
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
  await refreshInstalledData(true);
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
  }, "settings");
}

function resetSettings() {
  return withLoading(async () => {
    const reset = await invoke<AppSettings>("reset_app_settings");
    state.settings = reset;
    applySettingsToState(reset);
    state.addonsPathExists = await invoke<boolean>("path_exists", { path: effectiveAddonsPath() });
  }, "settings");
}

function ensureHttpCacheStatsLoaded() {
  if (state.httpCacheStatsLoaded || isOperation("cache")) return;
  void loadHttpCacheStats();
}

function loadHttpCacheStats() {
  return withLoading(async () => {
    state.httpCacheStats = await invoke<HttpCacheStatsResponse>("get_http_cache_stats");
    state.httpCacheStatsLoaded = true;
  }, "cache");
}

function clearHttpCache() {
  return withLoading(async () => {
    state.httpCacheStats = await invoke<HttpCacheStatsResponse>("clear_http_cache");
    state.httpCacheStatsLoaded = true;
  }, "cache");
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
    external: '<path d="M15 3h6v6"/><path d="M10 14 21 3"/><path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/>',
    folder: '<path d="M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7l-2-2H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2Z"/><path d="M2 10h20"/>',
    rotate: '<path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 3v6h6"/>',
    target: '<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="3"/><path d="M12 2v3"/><path d="M12 19v3"/><path d="M2 12h3"/><path d="M19 12h3"/>',
  };
  return `<svg class="button-icon" viewBox="0 0 24 24" aria-hidden="true">${paths[name]}</svg>`;
}

function plainEsoText(value: string) {
  return stripEsoMarkup(value);
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
    loadInstalled(false);
  }
});
