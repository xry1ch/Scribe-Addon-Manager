import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { renderEsoMarkup, renderInlineEsoMarkup, stripEsoMarkup } from "./bbcode";
import { shouldShowInstalledAddon, shouldShowSearchAddon } from "./libraryFilters";
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
  BackupInspection,
  BackupResult,
  CachedImageResponse,
  CheckAddonsResponse,
  ClearSavedVariablesResponse,
  DependencyPlan,
  AddonDependencyStatus,
  HttpCacheStatsResponse,
  ImportExistingAddonsResponse,
  InstallRemoteAddonResponse,
  InstalledAddonsResponse,
  InstalledAddonDependenciesResponse,
  LocalAddon,
  MatchResult,
  PlanRemoteInstallResponse,
  PlanUpdateAllResponse,
  PlanUpdatesResponse,
  LinkInstalledAddonToRemoteResponse,
  RemoteCategory,
  RemoteAddonDetailsWithLocalStateResponse,
  RemoteMatchCandidate,
  RemoteMatchCandidatesResponse,
  RemoveInstalledAddonResponse,
  RestoreResult,
  SingleUpdateApplyResponse,
  SingleUpdatePlanResponse,
  UpdateAllAction,
} from "./types";

type Tab = "installed" | "search" | "settings";
type DetailsTab = "info" | "changelog" | "dependencies";
type SettingsSection = "folders" | "downloads" | "display" | "cache";
type InstalledFilter = "all" | "update" | "unknown" | "current";
type InstalledSort = "name" | "updated" | "downloads" | "status";
type SearchMode = "most_downloaded" | "recent";
type IconName = "check" | "external" | "folder" | "installed" | "rotate" | "search" | "settings" | "target";
type AddonContextAction = "uninstall" | "clear-savedvariables" | "open-folder";
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
  | "dependency-install"
  | "resolve-search"
  | "resolve-link"
  | "resolve-reinstall"
  | "update-apply"
  | "remove-apply"
  | "savedvariables-clear"
  | "manual-backup"
  | "backup-restore"
  | "update-all-apply";

interface AppState {
  tab: Tab;
  path: string;
  startupViewReady: boolean;
  startupFatalError: string | null;
  loading: boolean;
  operation: OperationKind | null;
  operationTarget: string | null;
  error: string | null;
  success: string | null;
  successDetail: string | null;
  warning: string | null;
  installed: InstalledAddonsResponse | null;
  searchQuery: string;
  activeSettingsSection: SettingsSection;
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
  searchPageSize: number;
  visibleSearchCount: number;
  totalSearchMatches: number;
  searchResults: AddonSummary[];
  selectedSummary: AddonSummary | null;
  selectedDetails: AddonDetails | null;
  detailsTab: DetailsTab;
  lightboxImageUrl: string | null;
  selectedLocal: LocalAddon | null;
  selectedMatch: MatchResult | null;
  selectedDependencies: InstalledAddonDependenciesResponse | null;
  selectedDependenciesLoading: boolean;
  selectedDependenciesError: string | null;
  updates: CheckAddonsResponse | null;
  updatePlan: PlanUpdatesResponse | null;
  includeUnknown: boolean;
  installPlan: PlanRemoteInstallResponse | null;
  installResult: InstallRemoteAddonResponse | null;
  forceUpdate: boolean;
  singleUpdatePhase: "preparing" | "updating" | null;
  singleUpdatePlan: SingleUpdatePlanResponse | null;
  singleUpdateResult: SingleUpdateApplyResponse | null;
  resolveLocal: LocalAddon | null;
  resolveCandidates: RemoteMatchCandidate[];
  resolveSelectedUid: string | null;
  resolveMessage: string | null;
  removeResult: RemoveInstalledAddonResponse | null;
  removeConfirmLocal: LocalAddon | null;
  removeSavedVariables: boolean;
  clearSavedVariablesResult: ClearSavedVariablesResponse | null;
  clearSavedVariablesConfirmLocal: LocalAddon | null;
  manualBackupConfirmOpen: boolean;
  manualBackupIncludeSavedVariables: boolean;
  manualBackupResult: BackupResult | null;
  manualBackupError: string | null;
  restoreZipPath: string | null;
  restoreInspection: BackupInspection | null;
  restoreAddons: boolean;
  restoreSavedVariables: boolean;
  restoreResult: RestoreResult | null;
  addonContextMenu: AddonContextMenuState | null;
  updateAllPlan: PlanUpdateAllResponse | null;
  updateAllResult: ApplyUpdateAllResponse | null;
  updateAllProgress: UpdateAllProgress | null;
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

interface AddonContextMenuState {
  folderName: string;
  x: number;
  y: number;
}

interface UpdateAllProgress {
  index: number;
  total: number;
  local_folder: string;
}

interface CategoryMeta {
  name: string;
  x: number;
  y: number;
}

const state: AppState = {
  tab: "installed",
  path: "",
  startupViewReady: false,
  startupFatalError: null,
  loading: false,
  operation: null,
  operationTarget: null,
  error: null,
  success: null,
  successDetail: null,
  warning: null,
  installed: null,
  searchQuery: "",
  activeSettingsSection: "folders",
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
  searchPageSize: 25,
  visibleSearchCount: 25,
  totalSearchMatches: 0,
  searchResults: [],
  selectedSummary: null,
  selectedDetails: null,
  detailsTab: "info",
  lightboxImageUrl: null,
  selectedLocal: null,
  selectedMatch: null,
  selectedDependencies: null,
  selectedDependenciesLoading: false,
  selectedDependenciesError: null,
  updates: null,
  updatePlan: null,
  includeUnknown: false,
  installPlan: null,
  installResult: null,
  forceUpdate: false,
  singleUpdatePhase: null,
  singleUpdatePlan: null,
  singleUpdateResult: null,
  resolveLocal: null,
  resolveCandidates: [],
  resolveSelectedUid: null,
  resolveMessage: null,
  removeResult: null,
  removeConfirmLocal: null,
  removeSavedVariables: false,
  clearSavedVariablesResult: null,
  clearSavedVariablesConfirmLocal: null,
  manualBackupConfirmOpen: false,
  manualBackupIncludeSavedVariables: false,
  manualBackupResult: null,
  manualBackupError: null,
  restoreZipPath: null,
  restoreInspection: null,
  restoreAddons: true,
  restoreSavedVariables: false,
  restoreResult: null,
  addonContextMenu: null,
  updateAllPlan: null,
  updateAllResult: null,
  updateAllProgress: null,
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
const ADDON_CONTEXT_MENU_WIDTH = 224;
const ADDON_CONTEXT_MENU_HEIGHT = 132;
const CONTEXT_MENU_MARGIN = 8;
const SEARCH_SCROLL_THRESHOLD_PX = 300;
const UPDATE_ALL_PROGRESS_EVENT = "scribe-update-all-progress";
const TEXT_INPUT_IDS = {
  setupAddonsPath: "scribe-setup-addons-path",
  installedFilter: "scribe-installed-filter",
  addonSearch: "scribe-addon-search",
  settingsAddonsPath: "scribe-addons-path",
  settingsBackupFolder: "scribe-backup-folder",
  settingsDownloadFolder: "scribe-download-folder",
} as const;
const noAutocompleteAttrs = [
  `autocomplete="off"`,
  `autocorrect="off"`,
  `autocapitalize="off"`,
  `spellcheck="false"`,
  `data-form-type="other"`,
  `data-lpignore="true"`,
  `data-1p-ignore="true"`,
].join(" ");
const disableDevToolsShortcuts = false;

let searchScrollContainer: HTMLElement | null = null;
let searchScrollHandler: (() => void) | null = null;

document.addEventListener("contextmenu", handleGlobalContextMenu);
document.addEventListener("pointerdown", (event) => {
  if (!state.addonContextMenu) return;
  const target = event.target;
  if (target instanceof Element && target.closest(".addon-context-menu")) return;
  closeAddonContextMenu();
});
window.addEventListener("blur", () => closeAddonContextMenu());
window.addEventListener("resize", () => closeAddonContextMenu());
window.addEventListener("keydown", preventProductionDevToolsShortcut, { capture: true });

window.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  if (state.addonContextMenu) {
    event.preventDefault();
    closeAddonContextMenu();
    return;
  }
  if (state.clearSavedVariablesConfirmLocal && !guardedOperationRunning()) {
    event.preventDefault();
    cancelClearSavedVariables();
    return;
  }
  if (state.manualBackupConfirmOpen && !guardedOperationRunning()) {
    event.preventDefault();
    cancelManualBackup();
    return;
  }
  if (state.restoreInspection && !guardedOperationRunning()) {
    event.preventDefault();
    cancelRestoreBackup();
    return;
  }
  if (state.removeConfirmLocal && !guardedOperationRunning()) {
    event.preventDefault();
    cancelRemoveAddon();
    return;
  }
  if (state.resolveLocal && !guardedOperationRunning()) {
    event.preventDefault();
    closeResolveRemoteMatch();
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
  unbindSearchScrollListener();

  if (!state.startupViewReady) {
    app.innerHTML = state.startupFatalError ? renderStartupError() : renderStartupLoader();
    bindStartupEvents();
    return;
  }

  if (state.needsInitialSetup) {
    app.innerHTML = renderInitialSetup();
    bindInitialSetupEvents();
    return;
  }

  app.innerHTML = `
    <main class="app-shell">
      <aside class="sidebar">
        <div class="brand sidebar-brand">
          ${brandMark()}
          <div>
            <h1>Scribe</h1>
            <p>ESO Addon Manager</p>
          </div>
        </div>
        <nav class="nav-list sidebar-nav" aria-label="Primary navigation">
          ${tabButton("installed", "Installed")}
          ${tabButton("search", "Search")}
        </nav>
        <div class="sidebar-spacer" aria-hidden="true"></div>
        <nav class="nav-list sidebar-bottom" aria-label="Settings navigation">
          ${tabButton("settings", "Settings")}
        </nav>
      </aside>
      <section class="content">
        ${state.error ? `<div class="banner error">${escapeHtml(state.error)}</div>` : ""}
        ${state.success ? `<div class="banner success compact-banner">${escapeHtml(state.success)}${state.successDetail ? `<p class="banner-helper">${escapeHtml(state.successDetail)}</p>` : ""}</div>` : ""}
        ${state.warning ? `<div class="banner warning">${escapeHtml(state.warning)}</div>` : ""}
        ${renderCurrentTab()}
        ${renderDetailsModal()}
        ${renderResolveRemoteMatchModal()}
        ${renderRemoveConfirmationModal()}
        ${renderClearSavedVariablesConfirmationModal()}
        ${renderManualBackupConfirmationModal()}
        ${renderRestoreBackupModal()}
        ${renderAddonContextMenu()}
      </section>
    </main>
  `;
  bindCommonEvents();
  bindTabEvents();
}

function renderStartupLoader() {
  return `
    <main id="startup-loader" class="startup-loader" role="status" aria-live="polite" aria-label="Loading Scribe">
      <div class="startup-loader-content">
        <span class="startup-logo-frame" aria-hidden="true">
          <img class="startup-logo" src="${escapeAttr(logoUrl)}" alt="" />
        </span>
        <div>
          <h1>Scribe</h1>
          <p class="startup-subtitle">ESO Addon Manager</p>
        </div>
        <span class="startup-shimmer" aria-hidden="true"></span>
        <p class="startup-loading-text">${escapeHtml(startupLoadingText())}</p>
      </div>
    </main>
  `;
}

function renderStartupError() {
  return `
    <main id="startup-loader" class="startup-loader startup-error-screen" role="alert">
      <div class="startup-loader-content">
        <span class="startup-logo-frame" aria-hidden="true">
          <img class="startup-logo" src="${escapeAttr(logoUrl)}" alt="" />
        </span>
        <div>
          <h1>Scribe</h1>
          <p class="startup-subtitle">ESO Addon Manager</p>
        </div>
        <div class="startup-error-box">
          <h2>Could not start Scribe</h2>
          <p>${escapeHtml(state.startupFatalError ?? "An unknown startup error occurred.")}</p>
          <button class="secondary startup-retry-button" id="retry-startup" type="button">Retry</button>
        </div>
      </div>
    </main>
  `;
}

function startupLoadingText() {
  return state.operation === "installed" ? "Preparing your addons..." : "Loading addon manager...";
}

function bindStartupEvents() {
  document.querySelector<HTMLButtonElement>("#retry-startup")?.addEventListener("click", () => {
    void initializeApp();
  });
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
          <label for="${TEXT_INPUT_IDS.setupAddonsPath}">AddOns path</label>
          <div class="field-with-action">
            ${textInput(TEXT_INPUT_IDS.setupAddonsPath, state.setupAddonsPath, {
              placeholder: "C:\\Users\\Name\\Documents\\Elder Scrolls Online\\live\\AddOns",
            })}
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
          <h2 id="remove-addon-title">Uninstall addon?</h2>
          <p>This will delete the addon folder from AddOns.</p>
          <p class="modal-path" title="${escapeAttr(local.folder_name)}">${escapeHtml(local.folder_name)}</p>
          <div class="modal-option">
            <label class="checkbox-line" for="remove-addon-savedvariables">
              <input ${checkboxInputAttrs("remove-addon-savedvariables", "scribe-remove-addon-savedvariables")} type="checkbox" ${state.removeSavedVariables ? "checked" : ""} ${disabledAttr()} />
              <span>Also delete SavedVariables</span>
            </label>
            <p class="setting-helper">SavedVariables store addon settings and account/character data. Leave this unchecked to keep your settings.</p>
          </div>
        </div>
        <div class="modal-actions">
          <button class="secondary" id="cancel-remove-addon" ${disabledAttr()}>Cancel</button>
          <button class="danger" id="confirm-remove-addon" ${disabledAttr()}>${loadingButtonContent("Uninstall", "Uninstalling...", "remove-apply")}</button>
        </div>
      </section>
    </div>
  `;
}

function renderClearSavedVariablesConfirmationModal() {
  const local = state.clearSavedVariablesConfirmLocal;
  if (!local) return "";
  return `
    <div class="modal-backdrop remove-confirmation" role="presentation">
      <section class="modal-panel remove-confirmation-panel" role="dialog" aria-modal="true" aria-labelledby="clear-savedvariables-title">
        <div class="modal-icon danger-icon">${icon("target")}</div>
        <div class="modal-content">
          <h2 id="clear-savedvariables-title">Clear SavedVariables?</h2>
          <p>This will delete this addon’s SavedVariables files. The addon folder will remain installed.</p>
          <p class="modal-path" title="${escapeAttr(local.folder_name)}">${escapeHtml(local.folder_name)}</p>
        </div>
        <div class="modal-actions">
          <button class="secondary" id="cancel-clear-savedvariables" ${disabledAttr()}>Cancel</button>
          <button class="danger" id="confirm-clear-savedvariables" ${disabledAttr()}>${loadingButtonContent("Clear SavedVariables", "Clearing...", "savedvariables-clear")}</button>
        </div>
      </section>
    </div>
  `;
}

function renderManualBackupConfirmationModal() {
  if (!state.manualBackupConfirmOpen) return "";
  return `
    <div class="modal-backdrop remove-confirmation" role="presentation">
      <section class="modal-panel remove-confirmation-panel" role="dialog" aria-modal="true" aria-labelledby="manual-backup-title">
        <div class="modal-icon">${icon("folder")}</div>
        <div class="modal-content">
          <h2 id="manual-backup-title">Create backup?</h2>
          <p>This will create a compressed ZIP backup in the selected backup folder.</p>
          <div class="modal-option">
            <label class="checkbox-line" for="manual-backup-savedvariables">
              <input ${checkboxInputAttrs("manual-backup-savedvariables", "scribe-manual-backup-savedvariables")} type="checkbox" ${state.manualBackupIncludeSavedVariables ? "checked" : ""} ${disabledAttr()} />
              <span>Include SavedVariables</span>
            </label>
            <p class="setting-helper">SavedVariables contain addon settings and account/character data.</p>
          </div>
        </div>
        <div class="modal-actions">
          <button class="secondary" id="cancel-manual-backup" ${disabledAttr()}>Cancel</button>
          <button class="primary" id="confirm-manual-backup" ${disabledAttr()}>${loadingButtonContent("Create backup", "Creating backup...", "manual-backup")}</button>
        </div>
      </section>
    </div>
  `;
}

function renderRestoreBackupModal() {
  const inspection = state.restoreInspection;
  if (!inspection || !state.restoreZipPath) return "";
  return `
    <div class="modal-backdrop remove-confirmation" role="presentation">
      <section class="modal-panel restore-preview-panel" role="dialog" aria-modal="true" aria-labelledby="restore-backup-title">
        <div class="modal-icon danger-icon">${icon("rotate")}</div>
        <div class="modal-content">
          <h2 id="restore-backup-title">Restore backup?</h2>
          <div class="restore-preview-grid">
            ${restorePreviewItem("Backup", inspection.backup_name)}
            ${restorePreviewItem("Created", formatBackupDate(inspection.created_at))}
            ${restorePreviewItem("Contains AddOns", yesNo(inspection.contains_addons))}
            ${restorePreviewItem("Contains SavedVariables", yesNo(inspection.contains_saved_variables))}
            ${restorePreviewItem("Estimated files", formatCount(inspection.file_count))}
            ${restorePreviewItem("Estimated size", formatBytesDisplay(inspection.total_bytes))}
          </div>
          ${pathDisplay(`Target AddOns folder: ${inspection.target_addons_folder}`)}
          ${inspection.contains_saved_variables ? pathDisplay(`Target SavedVariables folder: ${inspection.target_saved_variables_folder}`) : ""}
          <div class="modal-option">
            <label class="checkbox-line" for="restore-backup-addons">
              <input ${checkboxInputAttrs("restore-backup-addons", "scribe-restore-backup-addons")} type="checkbox" ${state.restoreAddons ? "checked" : ""} ${disabledAttr()} />
              <span>Restore AddOns</span>
            </label>
            <label class="checkbox-line ${inspection.contains_saved_variables ? "" : "disabled-option"}" for="restore-backup-savedvariables">
              <input ${checkboxInputAttrs("restore-backup-savedvariables", "scribe-restore-backup-savedvariables")} type="checkbox" ${state.restoreSavedVariables ? "checked" : ""} ${!inspection.contains_saved_variables || state.loading ? "disabled" : ""} />
              <span>Restore SavedVariables</span>
            </label>
          </div>
          <p>Restoring AddOns will replace the current AddOns folder. Create a backup first if you want to keep the current state.</p>
          ${inspection.warnings.length > 0 ? `<p class="restore-warning">${escapeHtml(inspection.warnings.join(" "))}</p>` : ""}
        </div>
        <div class="modal-actions">
          <button class="secondary" id="cancel-restore-backup" ${disabledAttr()}>Cancel</button>
          <button class="danger" id="confirm-restore-backup" ${disabledAttr()}>${loadingButtonContent("Confirm Restore", "Restoring...", "backup-restore")}</button>
        </div>
      </section>
    </div>
  `;
}

function renderResolveRemoteMatchModal() {
  const local = state.resolveLocal;
  if (!local) return "";
  const selected = selectedResolveCandidate();
  const searchLoading = isOperation("resolve-search", local.folder_name);
  const actionDisabled = state.loading || !selected ? "disabled" : "";
  const closeDisabled = guardedOperationRunning() ? "disabled" : "";
  const resolveError = state.error && state.resolveLocal ? state.error : null;
  return `
    <div class="modal-backdrop resolve-modal-backdrop" role="presentation">
      <section class="modal-panel resolve-modal-panel" role="dialog" aria-modal="true" aria-labelledby="resolve-remote-match-title">
        <div class="resolve-modal-header">
          <div class="modal-icon resolve-icon">${icon("search")}</div>
          <h2 id="resolve-remote-match-title">Resolve remote match</h2>
        </div>
        <div class="resolve-modal-content">
          <div class="resolve-local-grid">
            ${resolveLocalItem("Local title", local.title ?? local.folder_name)}
            ${resolveLocalItem("Folder", local.folder_name)}
            ${resolveLocalItem("Author", local.author ?? "Author unknown")}
            ${resolveLocalItem("Local version", local.display_version ?? "-")}
          </div>
          ${resolveError ? `<div class="banner error compact-banner resolve-inline-error">${escapeHtml(resolveError)}</div>` : ""}
          ${!resolveError && state.resolveMessage && state.resolveCandidates.length > 0 ? `<div class="banner warning compact-banner">${escapeHtml(state.resolveMessage)}</div>` : ""}
          ${searchLoading ? renderResolveCandidateSkeletons() : resolveError && state.resolveCandidates.length === 0 ? "" : renderResolveCandidates()}
        </div>
        <div class="modal-actions resolve-modal-actions">
          <button class="secondary" id="cancel-resolve-remote-match" ${closeDisabled}>Close</button>
          <button class="primary" id="link-resolve-candidate" ${actionDisabled}>${loadingButtonContent("Link only", "Linking...", "resolve-link", local.folder_name)}</button>
          <button class="warning-action" id="reinstall-resolve-candidate" ${actionDisabled}>${loadingButtonContent("Reinstall from selected", "Reinstalling...", "resolve-reinstall", local.folder_name)}</button>
        </div>
      </section>
    </div>
  `;
}

function resolveLocalItem(label: string, value: string) {
  return `
    <div class="resolve-local-item">
      <span>${escapeHtml(label)}</span>
      <strong title="${escapeAttr(value)}">${renderInlineEsoMarkup(value)}</strong>
    </div>
  `;
}

function renderResolveCandidateSkeletons() {
  return `
    <div class="resolve-candidate-list" aria-label="Loading candidate matches">
      ${Array.from({ length: 3 }, () => renderResolveCandidateSkeleton()).join("")}
    </div>
  `;
}

function renderResolveCandidateSkeleton() {
  return `
    <article class="resolve-candidate resolve-candidate-skeleton skeleton-card" aria-hidden="true">
      ${skeletonIcon()}
      <div class="resolve-candidate-main">
        <div class="skeleton-stack skeleton-title-stack">
          ${skeletonLine("skeleton-line-title")}
          ${skeletonLine("skeleton-line-medium")}
        </div>
        ${renderSkeletonMetaGrid(3, "resolve-candidate-meta-row")}
        ${skeletonLine("skeleton-line-long")}
      </div>
      <div class="resolve-candidate-actions">
        ${skeletonLine("skeleton-chip")}
        ${skeletonButton()}
      </div>
    </article>
  `;
}

function renderResolveCandidates() {
  const candidates = state.resolveCandidates;
  if (candidates.length === 0) {
    return `<div class="resolve-candidate-list resolve-empty-list">${emptyState("No remote matches found.", "Try searching manually from the Search page.")}</div>`;
  }
  return `
    <div class="resolve-candidate-list" role="radiogroup" aria-label="Remote match candidates">
      ${candidates.map(renderResolveCandidate).join("")}
    </div>
  `;
}

function renderResolveCandidate(candidate: RemoteMatchCandidate) {
  const selected = state.resolveSelectedUid === candidate.remote_uid;
  const selectedClass = selected ? "is-selected" : "";
  const category = resolveCandidateCategory(candidate);
  const remoteName = candidate.remote_name?.trim() || candidate.remote_uid;
  const author = candidate.remote_author?.trim() || "Author unknown";
  const websiteDisabled = candidate.remote_info_url ? "" : "disabled";
  return `
    <article class="resolve-candidate ${selectedClass}" role="radio" aria-checked="${selected ? "true" : "false"}" aria-disabled="${state.loading ? "true" : "false"}" tabindex="${state.loading ? "-1" : "0"}" data-resolve-candidate="${escapeAttr(candidate.remote_uid)}">
      ${CategoryIcon(category)}
      <div class="resolve-candidate-main">
        <div class="resolve-candidate-title-row">
          <h3>${renderInlineEsoMarkup(remoteName)}</h3>
          <span class="resolve-confidence">${escapeHtml(resolveConfidenceLabel(candidate.confidence))}</span>
        </div>
        <p class="resolve-candidate-subtitle">${escapeHtml(plainEsoText(author))} &middot; ${escapeHtml(category.name)}</p>
        <div class="resolve-candidate-meta-row">
          ${resolveCandidateMeta("Version", candidate.remote_version)}
          ${resolveCandidateMeta("Downloads", formatCount(candidate.remote_downloads))}
          ${resolveCandidateMeta("Updated", candidate.remote_updated_display)}
        </div>
        <p class="resolve-candidate-reason">${escapeHtml(resolveReasonLabel(candidate.reason))}</p>
      </div>
      <div class="resolve-candidate-actions">
        <span class="resolve-selected-indicator" aria-hidden="true">${selected ? icon("check") : ""}</span>
        <button class="secondary small" data-resolve-website="${escapeAttr(candidate.remote_info_url ?? "")}" ${websiteDisabled}>${icon("external")} Website</button>
      </div>
    </article>
  `;
}

function resolveConfidenceLabel(confidence: RemoteMatchCandidate["confidence"]) {
  if (confidence === "very-high") return "Very high match";
  if (confidence === "high") return "High match";
  if (confidence === "medium") return "Possible match";
  return "Low confidence";
}

function resolveCandidateMeta(label: string, value: string | null | undefined) {
  return `
    <span class="resolve-candidate-meta-item">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value?.trim() || "-")}</strong>
    </span>
  `;
}

function resolveCandidateCategory(candidate: RemoteMatchCandidate): CategoryMeta {
  const rawCategory = candidate.remote_category?.trim() || null;
  const id = rawCategory && /^\d+$/.test(rawCategory) ? rawCategory : null;
  const name = rawCategory && !id ? rawCategory : null;
  if (id && categoryKeyById[id]) return categoryMeta(null, id, false);
  if (name) return categoryMeta(name, null, false);
  return { ...categoryIconByKey.misc, name: "Unknown category" };
}

function resolveReasonLabel(reason: string) {
  const normalized = reason.toLowerCase().replace(/[-_/]+/g, " ");
  if (normalized.includes("exact normalized title") && normalized.includes("same author")) return "Exact name and author match";
  if (normalized.includes("exact normalized folder") && normalized.includes("same author")) return "Folder name and author match";
  if (normalized.includes("exact normalized title")) return "Exact name match";
  if (normalized.includes("same author") && normalized.includes("fuzzy")) return "Similar name and same author";
  if (normalized.includes("fuzzy title")) return "Similar name";
  if (normalized.includes("folder matches") || normalized.includes("folder similarity") || normalized.includes("remote name")) return "Folder name match";
  if (normalized.includes("same author")) return "Same author";
  return reason.trim() || "Possible match";
}

function selectedResolveCandidate() {
  return state.resolveCandidates.find((candidate) => candidate.remote_uid === state.resolveSelectedUid) ?? null;
}

function restorePreviewItem(label: string, value: string) {
  return `
    <div class="restore-preview-item">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
    </div>
  `;
}

function renderAddonContextMenu() {
  const menu = state.addonContextMenu;
  if (!menu) return "";
  return `
    <div class="addon-context-menu" role="menu" aria-label="Addon actions" style="left: ${menu.x}px; top: ${menu.y}px;">
      <button type="button" role="menuitem" data-addon-context-action="uninstall">Uninstall</button>
      <button type="button" role="menuitem" data-addon-context-action="clear-savedvariables">Clear SavedVariables</button>
      <button type="button" role="menuitem" data-addon-context-action="open-folder">Open in folder</button>
    </div>
  `;
}

function renderCurrentTab() {
  if (state.tab === "installed") return renderInstalled();
  if (state.tab === "search") return renderSearch();
  return renderSettings();
}

function tabButton(tab: Tab, label: string) {
  const icons: Record<Tab, IconName> = {
    installed: "installed",
    search: "search",
    settings: "settings",
  };
  return `<button class="nav-button ${state.tab === tab ? "active" : ""} ${tab === "settings" ? "settings-nav-button" : ""}" data-tab="${tab}">${icon(icons[tab])}<span>${escapeHtml(label)}</span></button>`;
}

function brandMark() {
  return `<span class="brand-mark"><img src="${escapeAttr(logoUrl)}" alt="" /></span>`;
}

function renderInstalled() {
  const view = installedView();
  const updateAllButton = shouldShowUpdateAllButton()
    ? `<button class="primary" id="apply-update-all-installed" title="Updates all detected update candidates." ${disabledAttr()}>${loadingButtonContent("Update All", updateAllButtonLoadingLabel(), "update-all-apply")}</button>`
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
        ${textInput(TEXT_INPUT_IDS.installedFilter, state.installedQuery, {
          placeholder: "Addon name, author, folder",
        })}
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
    ${renderUpdateAllProgress()}
    ${renderUpdateAllResult()}
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
  return installedItems().some(isActionableInstalledUpdate);
}

function isActionableInstalledUpdate(item: InstalledViewModel) {
  return isActionableUpdate(item.match);
}

function isActionableUpdate(match: MatchResult | null | undefined) {
  return match?.update_confidence === "reliable-update";
}

function renderInstalledCard(item: InstalledViewModel) {
  const addon = item.addon;
  const remote = item.match?.remote ?? null;
  const status = installedStatus(item.match, addon);
  const title = remote?.name ?? addon.title ?? addon.folder_name;
  const category = categoryMeta(remote?.category_name ?? null, remote?.category_id ?? null, addon.is_library === true || remote?.is_library === true);
  const author = remote?.author_name ?? addon.author ?? null;
  const statusNote = installedStatusNote(status, item.match);
  return `
    <article class="addon-card clickable ${cardStatusClass(status.kind)}" data-installed-folder="${escapeAttr(addon.folder_name)}" data-addon-context-menu="true">
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
      <div class="card-actions">${renderInstalledCardActions(item, status)}</div>
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
  const hasQuery = isTypedSearchActive();
  const hasCategoryFilter = state.searchCategoryId.trim().length > 0;
  const showSearchSkeleton = isSearchLoading() || (!state.searchLoaded && !state.error);
  return `
    ${pageHeader("Search", "Discover addons from remote metadata.", "")}
    <section class="control-panel search-controls ${hasQuery ? "typed-search-controls" : ""}">
      <div class="field search-mode-field">
        <span>Mode</span>
        <div class="chip-row search-mode-buttons" role="group" aria-label="Search mode">
          ${searchModeButton("most_downloaded", "Most Downloaded")}
          ${searchModeButton("recent", "Recent")}
        </div>
      </div>
      <label class="field search-query-field">
        <span>Search term</span>
        <div class="field-with-action">
          ${textInput(TEXT_INPUT_IDS.addonSearch, state.searchQuery, {
            placeholder: "Addon name, author, or keyword",
          })}
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
      ${hasQuery ? "" : renderSearchLimitControl()}
    </section>
    ${state.searchCategoryWarning ? `<div class="banner warning compact-banner">${escapeHtml(state.searchCategoryWarning)}</div>` : ""}
    ${state.searchLoaded || showSearchSkeleton ? renderSearchResultSummary(showSearchSkeleton, hasCategoryFilter) : ""}
    <section class="addon-list" id="search-list">${renderSearchList(showSearchSkeleton)}</section>
    ${renderSearchIncrementStatus(showSearchSkeleton)}
  `;
}

function renderSearchLimitControl() {
  return `
    <label class="field limit-field">
      <span>Limit</span>
      <select id="search-limit" ${disabledAttr()}>
        ${[10, 25, 50, 100].map((value) => `<option value="${value}" ${state.searchLimit === value ? "selected" : ""}>${value}</option>`).join("")}
      </select>
    </label>
  `;
}

function renderSearchResultSummary(showSearchSkeleton: boolean, hasCategoryFilter = state.searchCategoryId.trim().length > 0) {
  const resultTitle = searchResultTitle();
  const categorySuffix = hasCategoryFilter ? ` - ${selectedCategoryName()}` : "";

  if (isTypedSearchActive()) {
    const subtext = showSearchSkeleton
      ? "Loading results..."
      : `Showing ${visibleSearchResults().length} of ${totalVisibleSearchMatches()} results${categorySuffix}`;
    return `
      <div class="result-caption search-result-summary" id="search-result-summary">
        <strong>${escapeHtml(resultTitle)}</strong>
        <span>${escapeHtml(subtext)}</span>
      </div>
    `;
  }

  return `<p class="result-caption" id="search-result-summary">${escapeHtml(resultTitle)}${escapeHtml(categorySuffix)}</p>`;
}

function searchResultTitle() {
  if (isTypedSearchActive()) return `Search results for "${state.searchAppliedQuery.trim()}"`;
  return state.searchMode === "recent" ? "Recent addons" : "Most downloaded addons";
}

function renderSearchList(showSearchSkeleton: boolean) {
  if (showSearchSkeleton) return renderSkeletonCards(6);
  if (!state.searchLoaded) return emptyState("Remote addons unavailable", "Resolve the error above, then refresh Search.");
  if (filteredSearchResults().length === 0) return emptyState("No matching addons", "No remote addons matched the current mode, category, and search filters.");
  return visibleSearchResults().map(renderSearchCard).join("");
}

function renderSearchIncrementStatus(showSearchSkeleton: boolean) {
  if (!isTypedSearchActive() || showSearchSkeleton || !state.searchLoaded || filteredSearchResults().length === 0) return "";
  return `<p class="search-load-status" id="search-load-status">${hasMoreSearchResults() ? "Scroll to load more" : "All results shown"}</p>`;
}

function isTypedSearchActive() {
  return state.searchAppliedQuery.trim().length > 0;
}

function visibleSearchResults() {
  const results = filteredSearchResults();
  if (!isTypedSearchActive()) return results;
  return results.slice(0, Math.min(state.visibleSearchCount, results.length));
}

function filteredSearchResults() {
  return state.searchResults.filter((addon) =>
    shouldShowSearchAddon(addon, {
      hideLibraries: state.settings?.hide_libraries_in_search ?? false,
      selectedCategoryId: state.searchCategoryId,
      categories: state.remoteCategories,
      query: state.searchAppliedQuery,
    }),
  );
}

function totalVisibleSearchMatches() {
  return filteredSearchResults().length;
}

function hasMoreSearchResults() {
  return isTypedSearchActive() && state.visibleSearchCount < totalVisibleSearchMatches();
}

function resetSearchPagination() {
  state.visibleSearchCount = state.searchPageSize;
}

function searchModeButton(mode: SearchMode, label: string) {
  return `<button class="chip search-mode-button ${state.searchMode === mode ? "active" : ""}" data-search-mode="${mode}" ${disabledAttr()}>${escapeHtml(label)}</button>`;
}

function selectedCategoryName() {
  return state.remoteCategories.find((category) => category.id === state.searchCategoryId)?.name ?? "Selected category";
}

function renderSearchCard(addon: AddonSummary) {
  const category = categoryMeta(addon.category_name, addon.category_id, addon.is_library);
  return `
    <article class="addon-card clickable${addon.installed ? " is-installed" : ""}" data-addon-id="${escapeAttr(addon.uid ?? "")}">
      ${CategoryIcon(category)}
      <div class="addon-main">
        <div class="addon-title-row">
          <div>
            <h3>${renderEsoText(addon.name ?? "Unnamed addon")}</h3>
            <p>${escapeHtml(addon.author_name ? `by ${plainEsoText(addon.author_name)}` : "Author unknown")}</p>
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
    local?.is_library === true || details?.is_library === true || match?.remote?.is_library === true || summary?.is_library === true,
  );
  const title = details?.name ?? match?.remote?.name ?? summary?.name ?? local?.title ?? local?.folder_name ?? "Addon Details";
  const author = details?.author_name ?? match?.remote?.author_name ?? summary?.author_name ?? local?.author ?? null;
  const installedVersion = local?.display_version ?? null;
  const remoteVersion = details?.version ?? match?.remote?.version ?? summary?.version ?? null;
  const downloads = details?.downloads ?? match?.remote?.downloads ?? summary?.downloads ?? null;
  const updated = details?.updated_display ?? match?.remote?.updated_display ?? summary?.updated_display ?? null;
  const statusNote = selectedDetailsStatusNote();
  const websiteUrl = selectedWebsiteUrl();
  const closeDisabled = guardedOperationRunning() ? "disabled" : "";
  const activeDetailsTab = selectedDetailsTab();
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
          ${activeDetailsTab === "dependencies" ? renderDependenciesTab() : activeDetailsTab === "changelog" ? renderChangelogTab() : renderAddonInfoTab()}
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
    ${renderSingleUpdateResult()}
    ${renderRemoveResult()}
  `;
}

function renderDetailsTabs() {
  const activeTab = selectedDetailsTab();
  const dependencyTab = shouldShowDependenciesTab()
    ? `<button class="details-tab ${activeTab === "dependencies" ? "active" : ""}" data-details-tab="dependencies" role="tab" aria-selected="${activeTab === "dependencies"}">Dependencies</button>`
    : "";
  return `
    <div class="details-tabs" role="tablist" aria-label="Addon details sections">
      <button class="details-tab ${activeTab === "info" ? "active" : ""}" data-details-tab="info" role="tab" aria-selected="${activeTab === "info"}">Addon Info</button>
      <button class="details-tab ${activeTab === "changelog" ? "active" : ""}" data-details-tab="changelog" role="tab" aria-selected="${activeTab === "changelog"}">Changelog</button>
      ${dependencyTab}
    </div>
  `;
}

function selectedDetailsTab(): DetailsTab {
  return state.detailsTab === "dependencies" && !shouldShowDependenciesTab() ? "info" : state.detailsTab;
}

function shouldShowDependenciesTab() {
  return Boolean(state.selectedLocal || selectedDependencyPlan());
}

function selectedDependencyPlan() {
  return state.installPlan?.dependency_plan ?? state.singleUpdateResult?.dependency_plan ?? null;
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

function renderDependenciesTab() {
  if (state.selectedLocal) return renderInstalledDependenciesTab();
  const plan = selectedDependencyPlan();
  if (plan) return renderPreviewDependenciesTab(plan);
  return emptyState("No dependencies found", "No dependency data is available for this addon.");
}

function renderInstalledDependenciesTab() {
  const dependencies = state.selectedDependencies;
  const warning = state.selectedDependenciesError ?? dependencies?.warning ?? null;

  if (state.selectedDependenciesLoading && !dependencies) {
    return `
      <section class="dependency-details">
        ${warning ? `<div class="banner warning compact-banner">${escapeHtml(warning)}</div>` : ""}
        <div class="dependency-status-section">
          <div class="dependency-status-heading">
            <h3>Required dependencies</h3>
            <span>Loading</span>
          </div>
          <div class="dependency-card-list">
            ${Array.from({ length: 3 }, () => renderSkeletonMiniRow()).join("")}
          </div>
        </div>
      </section>
    `;
  }

  if (!dependencies) {
    return `
      <section class="dependency-details">
        ${warning ? `<div class="banner warning compact-banner">${escapeHtml(warning)}</div>` : ""}
        ${emptyState("No dependencies found", "Dependency data is not available for this addon.")}
      </section>
    `;
  }

  return renderDependencyStatusSections(
    dependencies.required_dependencies,
    dependencies.optional_dependencies,
    warning,
  );
}

function renderPreviewDependenciesTab(plan: DependencyPlan) {
  const required = plan.required_dependencies;
  const optional = plan.optional_dependencies;
  if (required.length === 0 && optional.length === 0) {
    return emptyState("No dependencies found", "No required or optional dependencies were found in this package.");
  }

  return `
    <section class="dependency-details">
      ${renderPreviewDependencySection("Required dependencies", required, "No required dependencies.", true)}
      ${renderPreviewDependencySection("Optional dependencies", optional, "No optional dependencies.", false)}
    </section>
  `;
}

function renderDependencyStatusSections(required: AddonDependencyStatus[], optional: AddonDependencyStatus[], warning: string | null) {
  if (required.length === 0 && optional.length === 0) {
    return `
      <section class="dependency-details">
        ${warning ? `<div class="banner warning compact-banner">${escapeHtml(warning)}</div>` : ""}
        ${emptyState("No dependencies found", "No dependencies found.")}
      </section>
    `;
  }

  return `
    <section class="dependency-details">
      ${warning ? `<div class="banner warning compact-banner">${escapeHtml(warning)}</div>` : ""}
      ${renderInstalledDependencySection("Required dependencies", required, "No required dependencies.")}
      ${renderInstalledDependencySection("Optional dependencies", optional, "No optional dependencies.")}
    </section>
  `;
}

function renderInstalledDependencySection(title: string, dependencies: AddonDependencyStatus[], emptyMessage: string) {
  return `
    <div class="dependency-status-section">
      <div class="dependency-status-heading">
        <h3>${escapeHtml(title)}</h3>
        <span>${dependencies.length === 0 ? "None" : `${dependencies.length} found`}</span>
      </div>
      ${
        dependencies.length === 0
          ? `<p class="muted-text">${escapeHtml(emptyMessage)}</p>`
          : `<div class="dependency-card-list">${dependencies.map(renderInstalledDependencyCard).join("")}</div>`
      }
    </div>
  `;
}

function renderPreviewDependencySection(
  title: string,
  dependencies: DependencyPlan["required_dependencies"],
  emptyMessage: string,
  required: boolean,
) {
  return `
    <div class="dependency-status-section">
      <div class="dependency-status-heading">
        <h3>${escapeHtml(title)}</h3>
        <span>${dependencies.length === 0 ? "None" : `${dependencies.length} found`}</span>
      </div>
      ${
        dependencies.length === 0
          ? `<p class="muted-text">${escapeHtml(emptyMessage)}</p>`
          : `<div class="dependency-card-list">${dependencies.map((dependency) => renderPreviewDependencyCard(dependency, required)).join("")}</div>`
      }
    </div>
  `;
}

function renderInstalledDependencyCard(dependency: AddonDependencyStatus) {
  const action = renderInstalledDependencyAction(dependency);
  return `
    <article class="dependency-card ${dependencyAvailabilityClass(dependency)}" style="--dependency-depth: ${dependencyDepthOffset(dependency)}">
      <span class="dependency-status-icon" aria-hidden="true">${dependencyAvailabilityIcon(dependency)}</span>
      <div class="dependency-card-main">
        <div class="dependency-card-title">
          <strong>${escapeHtml(dependency.name)}</strong>
          <span>${escapeHtml(dependencyAvailabilityText(dependency))}</span>
        </div>
        ${renderInstalledDependencyLines(dependency)}
      </div>
      ${action ? `<div class="dependency-card-actions">${action}</div>` : ""}
    </article>
  `;
}

function renderPreviewDependencyCard(dependency: DependencyPlan["required_dependencies"][number], required: boolean) {
  return `
    <article class="dependency-card ${previewDependencyClass(dependency, required)}" style="--dependency-depth: ${dependencyDepthOffset(dependency)}">
      <span class="dependency-status-icon" aria-hidden="true">${previewDependencyIcon(dependency, required)}</span>
      <div class="dependency-card-main">
        <div class="dependency-card-title">
          <strong>${escapeHtml(dependency.name)}</strong>
          <span>${escapeHtml(dependencyStatusText(dependency))}</span>
        </div>
        ${renderPreviewDependencyLines(dependency)}
      </div>
    </article>
  `;
}

function renderInstalledDependencyLines(dependency: AddonDependencyStatus) {
  const installedName = plainEsoText(dependency.installed_title ?? dependency.installed_folder ?? dependency.name);
  const remoteName = plainEsoText(dependency.remote_name ?? dependency.name);
  const lines = [
    dependencyLine(dependencyRelationText(dependency)),
    dependency.constraint ? dependencyLine(`Requires ${dependency.constraint}`) : "",
    dependency.installed ? dependencyLine(compactDependencySummary("Installed", installedName, formatInstalledDependencyVersion(dependency.installed_version))) : "",
    dependency.remote_uid || dependency.remote_name ? dependencyLine(compactDependencySummary("Remote", remoteName, dependency.remote_version)) : "",
  ].filter(Boolean);

  if (lines.length === 1) {
    const fallback =
      dependency.status === "ambiguous"
        ? "Multiple remote matches"
        : dependency.status === "circular"
          ? "Circular dependency"
          : dependency.status === "max-depth"
            ? "Max depth reached"
            : dependency.status === "unknown"
              ? "Remote lookup unavailable"
              : "No remote match";
    lines.push(dependencyLine(fallback));
  }

  return `<div class="dependency-lines">${lines.join("")}</div>`;
}

function renderPreviewDependencyLines(dependency: DependencyPlan["required_dependencies"][number]) {
  const lines = [
    dependencyLine(dependencyRelationText(dependency)),
    dependency.constraint ? dependencyLine(`Requires ${dependency.constraint}`) : "",
    dependency.installed_folder ? dependencyLine(compactDependencySummary("Installed", dependency.installed_title ?? dependency.installed_folder, dependency.installed_version)) : "",
    dependency.remote_name ? dependencyLine(compactDependencySummary("Remote", dependency.remote_name, dependency.remote_version)) : "",
    dependency.bundled_folder ? dependencyLine(compactDependencySummary("Bundled", dependency.bundled_folder, null)) : "",
  ].filter(Boolean);

  if (lines.length === 1) {
    lines.push(dependencyLine(dependencyDetailText(dependency)));
  }

  return `<div class="dependency-lines">${lines.join("")}</div>`;
}

function dependencyLine(value: string) {
  return `
    <p class="dependency-line">${escapeHtml(value)}</p>
  `;
}

function compactDependencySummary(label: string, name: string, version: string | null | undefined) {
  return `${label}: ${name}${version ? ` · ${version}` : ""}`;
}

function dependencyDepthOffset(dependency: { depth?: number }) {
  return Math.max(0, (dependency.depth ?? 1) - 1);
}

function dependencyRelationText(dependency: { relation?: string; parent?: string | null; depth?: number; required?: boolean }) {
  const relation = dependency.relation === "optional" || dependency.required === false ? "Optional" : "Required";
  const parent = dependency.parent?.trim();
  if (parent && (dependency.depth ?? 1) > 1) {
    return `${relation === "Optional" ? "Optional for" : "Required by"} ${plainEsoText(parent)}`;
  }
  return relation;
}

function formatInstalledDependencyVersion(version: string | null) {
  const value = version?.trim();
  if (!value) return null;
  return value.toLowerCase().startsWith("v") ? value : `v${value}`;
}

function renderInstalledDependencyAction(dependency: AddonDependencyStatus) {
  if (dependency.installed && dependency.installed_folder) {
    return `<button class="secondary small" data-open-installed-dependency="${escapeAttr(dependency.installed_folder)}" ${disabledAttr()}>Open details</button>`;
  }
  if (dependency.status === "missing" && dependency.remote_uid) {
    return `<button class="primary small" data-install-dependency="${escapeAttr(dependency.remote_uid)}" ${disabledAttr()}>${loadingButtonContent("Install dependency", "Installing...", "dependency-install", dependency.remote_uid)}</button>`;
  }
  if (dependency.remote_uid) {
    return `<button class="secondary small" data-open-remote-dependency="${escapeAttr(dependency.remote_uid)}" ${disabledAttr()}>Details</button>`;
  }
  return "";
}

function dependencyAvailabilityText(dependency: AddonDependencyStatus) {
  if (dependency.status === "installed") return "Installed";
  if (dependency.status === "missing") return "Missing";
  if (dependency.status === "ambiguous") return "Ambiguous";
  if (dependency.status === "circular") return "Circular";
  if (dependency.status === "max-depth") return "Max depth";
  return "Unknown";
}

function dependencyAvailabilityClass(dependency: AddonDependencyStatus) {
  if (dependency.status === "installed") return "is-installed";
  if (dependency.status === "missing" && dependency.required) return "is-required-missing";
  if (dependency.status === "missing") return "is-optional-missing";
  if (dependency.status === "ambiguous" || dependency.status === "circular" || dependency.status === "max-depth") return "is-ambiguous";
  return "is-unknown";
}

function dependencyAvailabilityIcon(dependency: AddonDependencyStatus) {
  if (dependency.status === "installed") return resultCheckIcon();
  if (dependency.status === "missing") return resultWarningIcon();
  if (dependency.status === "ambiguous" || dependency.status === "circular" || dependency.status === "max-depth") return resultWarningIcon();
  return neutralDependencyIcon();
}

function previewDependencyClass(dependency: DependencyPlan["required_dependencies"][number], required: boolean) {
  if (dependency.status === "already-installed") return "is-installed";
  if (dependency.status === "will-install") return "is-planned";
  if (dependency.status === "ambiguous" || dependency.status === "circular" || dependency.status === "max-depth") return "is-ambiguous";
  if (dependency.status === "unresolved" || dependency.status === "not-installed") {
    return required ? "is-required-missing" : "is-optional-missing";
  }
  return "is-unknown";
}

function previewDependencyIcon(dependency: DependencyPlan["required_dependencies"][number], required: boolean) {
  if (dependency.status === "already-installed") return resultCheckIcon();
  if (dependency.status === "will-install") return neutralDependencyIcon();
  if (dependency.status === "ambiguous" || dependency.status === "circular" || dependency.status === "max-depth" || dependency.status === "unresolved" || (required && dependency.status === "not-installed")) {
    return resultWarningIcon();
  }
  return neutralDependencyIcon();
}

function neutralDependencyIcon() {
  return `<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9"></circle><path d="M12 8v5"></path><path d="M12 16h.01"></path></svg>`;
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
    ? `<button class="danger" id="remove-addon" ${disabledAttr()}>${loadingButtonContent("Uninstall", "Uninstalling...", "remove-apply")}</button>`
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
      if (hasRequiredDependencyIssues(state.installPlan.dependency_plan)) return "";
      return `<button class="danger" id="confirm-install" ${disabledAttr()}>${loadingButtonContent("Confirm Install", "Installing...", "install-apply")}</button>`;
    }
    if (!state.installResult) {
      return `<button class="primary" id="plan-install" ${disabledAttr()}>${loadingButtonContent("Install", "Preparing install...", "install-plan")}</button>`;
    }
    return "";
  }

  if (!match) return "";
  const target = match.local.folder_name;
  if (isActionableUpdate(match)) {
    return `<button class="primary" data-apply-update-target="${escapeAttr(target)}" ${disabledAttr()}>${loadingButtonContent("Update", singleUpdateButtonLoadingLabel(target), "update-apply", target)}</button>`;
  }
  if (state.forceUpdate && ["matched", "unknown-update", "local-newer"].includes(match.status)) {
    return `<button class="secondary" data-apply-update-target="${escapeAttr(target)}" ${disabledAttr()}>${loadingButtonContent("Reinstall", singleUpdateButtonLoadingLabel(target), "update-apply", target)}</button>`;
  }
  return "";
}

function selectedDetailsStatusNote() {
  const local = state.selectedLocal;
  const match = state.selectedMatch;
  if (state.removeResult?.removed_addon) return "Addon uninstalled";
  if (!local) {
    if (isOperation("install-apply")) return "Installing...";
    if (state.installPlan && !state.installResult) return "Install preview ready";
    if (state.installResult) return "Installed successfully";
    return "Not installed locally";
  }
  if (state.singleUpdateResult) return "Update completed";
  const status = installedStatus(match, local);
  const localName = plainEsoText(local.title?.trim() || local.folder_name);
  const version = local.display_version ? `, version ${local.display_version}` : "";
  const statusNote = installedStatusNote(status, match);
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
    ${pageHeader("Settings", "Choose where Scribe manages addons and how downloads are handled.", `
      <div class="toolbar-actions">
        <button class="secondary icon-button" id="reset-settings" ${disabledAttr()}>${loadingButtonContent(`${icon("rotate")} Reset`, "Loading...", "settings")}</button>
        <button class="primary icon-button" id="save-settings" ${disabledAttr()}>${loadingButtonContent(`${icon("check")} Save`, "Loading...", "settings")}</button>
      </div>
    `)}
    ${addonsMissing ? `<div class="banner error">Configured AddOns path does not exist: ${escapeHtml(settings?.addons_dir_override ?? "")}</div>` : ""}
    <section class="settings-layout">
      ${renderSettingsNavigation()}
      ${renderActiveSettingsSection(settings)}
    </section>
  `;
}

function renderSettingsNavigation() {
  const sections: Array<{ id: SettingsSection; label: string; helper: string }> = [
    { id: "folders", label: "Folders", helper: "Addon and backup locations" },
    { id: "downloads", label: "Downloads", helper: "ZIP files and update visibility" },
    { id: "display", label: "Display", helper: "Library visibility" },
    { id: "cache", label: "Cache", helper: "Temporary browsing data" },
  ];

  return `
    <nav class="settings-section-nav" aria-label="Settings sections" role="tablist">
      ${sections.map((section) => settingsSectionButton(section)).join("")}
    </nav>
  `;
}

function settingsSectionButton(section: { id: SettingsSection; label: string; helper: string }) {
  const active = state.activeSettingsSection === section.id;
  return `
    <button
      type="button"
      id="settings-section-tab-${escapeAttr(section.id)}"
      class="settings-section-button ${active ? "active" : ""}"
      data-settings-section="${escapeAttr(section.id)}"
      role="tab"
      aria-selected="${active ? "true" : "false"}"
      aria-controls="settings-section-panel"
      ${disabledAttr()}
    >
      <span>${escapeHtml(section.label)}</span>
      <small>${escapeHtml(section.helper)}</small>
    </button>
  `;
}

function renderActiveSettingsSection(settings: AppSettings | null) {
  if (state.activeSettingsSection === "downloads") return renderDownloadsSettings(settings);
  if (state.activeSettingsSection === "display") return renderDisplaySettings(settings);
  if (state.activeSettingsSection === "cache") return renderHttpCacheSettings();
  return renderFolderSettings(settings);
}

function renderFolderSettings(settings: AppSettings | null) {
  return `
    <section class="panel settings-detail-panel" id="settings-section-panel" role="tabpanel" aria-labelledby="settings-section-tab-folders">
      <div class="settings-detail-heading">
        <div>
          <h3>Folders</h3>
          <p>Leave blank to let Scribe choose automatically.</p>
        </div>
      </div>
      <div class="settings-detail-body">
        ${settingField("AddOns folder", TEXT_INPUT_IDS.settingsAddonsPath, settings?.addons_dir_override ?? "", {
          browse: true,
          helper: "Where ESO loads your addons from.",
          placeholder: "Auto-detect AddOns folder",
        })}
        ${settingField("Backup folder", TEXT_INPUT_IDS.settingsBackupFolder, settings?.backup_dir_override ?? "", {
          browse: true,
          helper: "Where manual backups are saved.",
          placeholder: "Choose backup folder",
        })}
        ${renderManualBackupSettings()}
      </div>
    </section>
  `;
}

function renderDownloadsSettings(settings: AppSettings | null) {
  return `
    <section class="panel settings-detail-panel" id="settings-section-panel" role="tabpanel" aria-labelledby="settings-section-tab-downloads">
      <div class="settings-detail-heading">
        <div>
          <h3>Downloads</h3>
          <p>Control where update packages go and which uncertain updates Scribe shows.</p>
        </div>
      </div>
      <div class="settings-detail-body">
        ${settingField("Download folder", TEXT_INPUT_IDS.settingsDownloadFolder, settings?.download_dir ?? "", {
          browse: true,
          helper: "Where ZIP files are saved when you keep downloads.",
          placeholder: "Use default download folder",
        })}
        ${settingToggle(
          "Keep downloaded ZIP files",
          "settings-keep-downloads",
          settings?.keep_downloads_default ?? false,
          "Useful if you want to reinstall addons without downloading again.",
        )}
        ${settingToggle(
          "Show uncertain updates",
          "settings-include-unknown",
          settings?.include_unknown_updates_default ?? false,
          "Shows addons where Scribe cannot confidently compare versions.",
        )}
      </div>
    </section>
  `;
}

function renderDisplaySettings(settings: AppSettings | null) {
  return `
    <section class="panel settings-detail-panel" id="settings-section-panel" role="tabpanel" aria-labelledby="settings-section-tab-display">
      <div class="settings-detail-heading">
        <div>
          <h3>Display</h3>
          <p>Keep addon lists focused by hiding libraries where you do not need them.</p>
        </div>
      </div>
      <div class="settings-detail-body">
        ${settingToggle(
          "Hide libraries in Search",
          "settings-hide-libraries-search",
          settings?.hide_libraries_in_search ?? false,
          "Library addons will be hidden from browsing results unless they are directly searched for.",
        )}
        ${settingToggle(
          "Hide installed libraries",
          "settings-hide-libraries-installed",
          settings?.hide_libraries_in_installed ?? false,
          "Installed libraries will be hidden unless Scribe finds an update for them.",
        )}
      </div>
    </section>
  `;
}

function renderManualBackupSettings() {
  if (!state.settings?.backup_dir_override) return "";
  return `
    <div class="backup-actions">
      <button class="secondary icon-button" id="create-manual-backup" ${disabledAttr()}>${loadingButtonContent(`${icon("folder")} Create backup`, "Creating backup...", "manual-backup")}</button>
      <button class="secondary icon-button" id="restore-backup" ${disabledAttr()}>${loadingButtonContent(`${icon("rotate")} Restore backup`, "Restoring...", "backup-restore")}</button>
    </div>
    ${renderManualBackupStatus()}
    ${renderRestoreBackupStatus()}
  `;
}

function renderManualBackupStatus() {
  if (state.manualBackupError) {
    return `
      <p class="backup-status backup-status-error">
        <span>${escapeHtml(state.manualBackupError)}</span>
        ${state.manualBackupResult ? `<button class="backup-inline-action" id="open-created-backup-folder" title="Open backup location" ${disabledAttr()}>Open</button>` : ""}
      </p>
    `;
  }
  if (!state.manualBackupResult) return "";
  const hasSkippedFiles = state.manualBackupResult.skipped_files_count > 0;
  const label = hasSkippedFiles ? "Backup created with warnings" : "Backup created";

  return `
    <div class="backup-status-group">
      <p class="backup-status ${hasSkippedFiles ? "backup-status-warning" : "backup-status-success"}">
        ${icon("check")}
        <span>${escapeHtml(label)}</span>
        <span aria-hidden="true">&middot;</span>
        <button class="backup-inline-action" id="open-created-backup-folder" title="Open backup location" ${disabledAttr()}>Open</button>
      </p>
      ${hasSkippedFiles ? `<p class="backup-status-note">Some files could not be copied because they were in use.</p>${renderBackupSkippedFiles(state.manualBackupResult)}` : ""}
    </div>
  `;
}

function renderBackupSkippedFiles(result: BackupResult) {
  if (result.skipped_files_count === 0) return "";
  return `
    <details class="result-details backup-skipped-details">
      <summary>Skipped files (${formatCount(result.skipped_files_count)})</summary>
      <div class="mini-list">
        ${result.skipped_files.map((file) => `
          <div class="mini-row">
            <strong title="${escapeAttr(file.relative_path)}">${escapeHtml(file.relative_path)}</strong>
            <span>${escapeHtml(file.reason)}</span>
          </div>
        `).join("")}
      </div>
      ${result.skipped_files_count >= 10 ? `<p class="setting-helper">Close ESO or Minion and retry if many files were skipped.</p>` : ""}
    </details>
  `;
}

function renderRestoreBackupStatus() {
  if (!state.restoreResult) return "";
  const details = [
    state.restoreResult.restored_addons ? "AddOns restored" : null,
    state.restoreResult.restored_saved_variables ? "SavedVariables restored" : null,
  ].filter(Boolean).join(" · ");
  return `
    <p class="backup-status backup-status-success">
      ${icon("check")}
      <span>Backup restored</span>
      ${details ? `<span aria-hidden="true">&middot;</span><span>${escapeHtml(details)}</span>` : ""}
    </p>
  `;
}

function renderHttpCacheSettings() {
  const stats = state.httpCacheStats;
  return `
    <section class="panel settings-detail-panel" id="settings-section-panel" role="tabpanel" aria-labelledby="settings-section-tab-cache">
      <div class="settings-detail-heading">
        <div>
          <h3>Cache</h3>
          <p>Cached addon data and images make browsing faster.</p>
        </div>
        <div class="settings-detail-actions">
          <button class="danger" id="clear-http-cache" ${disabledAttr()}>${loadingButtonContent("Clear cache", "Clearing...", "cache")}</button>
        </div>
      </div>
      <div class="settings-detail-body">
        <div class="cache-summary">
          <span>Cache size</span>
          <strong>${escapeHtml(stats?.size_display ?? (state.httpCacheStatsLoaded ? "0 B" : "Loading..."))}</strong>
        </div>
        <p class="setting-helper">Clearing the cache will not remove installed addons or settings.</p>
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
            <p>All validated addon folders and required libraries are new installs.</p>
          </div>
        </div>
        ${renderPlanItems(plan.plan.items)}
        ${renderDependencyPlan(plan.dependency_plan)}
      </section>
    `;
  }
  const requiresReplacementReview = hasReplacementPlanItems(plan);
  const requiredDependencyIssues = hasRequiredDependencyIssues(plan.dependency_plan);
  const dependencyReplacementReview = hasDependencyReplacementItems(plan.dependency_plan);
  const hasInstallableItems = hasInstallablePlanItems(plan);
  const bannerClass =
    requiresReplacementReview || requiredDependencyIssues || dependencyReplacementReview || !hasInstallableItems || hasSkippedPlanItems(plan)
      ? "warning"
      : "info";
  const bannerText = requiredDependencyIssues
    ? "Some required dependencies could not be resolved safely. Install is blocked until those dependencies are resolved."
    : dependencyReplacementReview
      ? "Review required: installing required libraries may replace existing addon folders after creating backups."
      : requiresReplacementReview
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
      ${renderDependencyPlan(plan.dependency_plan)}
    </section>
  `;
}

function renderInstallResult() {
  const result = state.installResult;
  if (!result) return "";
  return renderCompactInstallResult(result);
}

function renderUpdateAllProgress() {
  if (!isOperation("update-all-apply")) return "";
  const progress = state.updateAllProgress;
  const message = progress
    ? `Updating ${progress.index} of ${progress.total}: ${plainEsoText(progress.local_folder)}`
    : "Preparing update...";
  return `
    <section class="panel result-panel">
      <div class="result-state">
        <span class="button-spinner" aria-hidden="true"></span>
        <div>
          <h3>${escapeHtml(message)}</h3>
          <p>Fetching fresh metadata, verifying the ZIP, and applying the update safely.</p>
        </div>
      </div>
    </section>
  `;
}

function updateAllNoUpdatesMessage(plan: PlanUpdateAllResponse) {
  const skipped = [
    plan.summary.skipped_current > 0 ? `${plan.summary.skipped_current} current` : null,
    plan.summary.skipped_local_newer > 0 ? `${plan.summary.skipped_local_newer} local newer` : null,
    plan.summary.skipped_unknown > 0 ? `${plan.summary.skipped_unknown} uncertain` : null,
    plan.summary.skipped_no_match > 0 ? `${plan.summary.skipped_no_match} without remote match` : null,
    plan.summary.skipped_ambiguous > 0 ? `${plan.summary.skipped_ambiguous} ambiguous` : null,
    plan.summary.skipped_libraries > 0 ? `${plan.summary.skipped_libraries} libraries` : null,
  ].filter(Boolean);

  if (skipped.length === 0) return "No actionable updates are available.";
  return `No actionable updates are available. Skipped: ${skipped.join(", ")}.`;
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
          ${metaItem("Reason", action.update_reason)}
        </div>
      </div>
    </article>
  `;
}

function renderUpdateAllResult() {
  const result = state.updateAllResult;
  if (!result) return "";
  const failure = result.failure;
  const updatedCount = result.results.length;
  const title = failure
    ? `Stopped at ${failure.local_folder}: ${failure.message}`
    : updatedCount > 0
      ? `Updated ${updatedCount} addon${updatedCount === 1 ? "" : "s"}`
      : "Already current";
  return `
    <section class="panel">
      <div class="banner ${failure ? "warning" : result.applied ? "success" : "warning"}">${escapeHtml(title)}</div>
      <div class="summary">
        ${summaryItem("Updated", updatedCount)}
        ${summaryItem("Previewed", result.summary.planned_updates)}
        ${summaryItem("Stopped", failure ? 1 : 0)}
      </div>
      ${result.results.length === 0 ? emptyState("No updates applied", "No addons were updated.") : result.results.map(renderUpdateAllResultCard).join("")}
    </section>
  `;
}

function renderUpdateAllResultCard(item: ApplyUpdateAllResponse["results"][number]) {
  return `
    <article class="addon-card compact-card">
      ${CategoryIcon(categoryMeta(item.remote_details.category_name, item.remote_details.category_id, item.remote_details.is_library))}
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
  note?: string | null;
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
    message: installSuccessMessage(result),
  });
}

function installSuccessMessage(result: InstallRemoteAddonResponse) {
  const requiredInstalled = result.dependency_plan.required_dependencies.filter((dependency) => dependency.status === "will-install").length;
  if (requiredInstalled > 0) {
    return `Installed 1 addon and ${requiredInstalled} required ${requiredInstalled === 1 ? "library" : "libraries"}.`;
  }
  return "The addon is ready to use in ESO.";
}

function renderCompactUpdateResult(result: SingleUpdateApplyResponse) {
  if (!result.applied) {
    return renderCompactResultPanel({
      kind: "warning",
      title: "Update finished without file changes",
      message: singleUpdateNoChangeMessage(result),
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
    title: result.removed_addon ? "Addon uninstalled" : "Uninstall finished without file changes",
    message: removeSuccessMessage(result),
    note: removeSavedVariablesStatusText(result),
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
          ${result.note ? `<p class="result-note">${escapeHtml(result.note)}</p>` : ""}
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

function renderDependencyPlan(plan: DependencyPlan | null) {
  if (!plan) return "";
  const hasRequired = plan.required_dependencies.length > 0;
  const hasOptional = plan.optional_dependencies.length > 0;
  if (!hasRequired && !hasOptional) return "";

  return `
    <div class="dependency-section">
      <div class="dependency-heading">
        <h4>Dependency tree</h4>
        <span>${requiredDependencySummary(plan)}</span>
      </div>
      ${
        hasRequired
          ? `<div class="dependency-list">${plan.required_dependencies.map(renderRequiredDependencyRow).join("")}</div>`
          : `<p class="muted-text">No required libraries were declared.</p>`
      }
      ${
        hasOptional
          ? `<details class="dependency-optional">
              <summary>Optional dependencies</summary>
              <div class="dependency-list">${plan.optional_dependencies.map(renderOptionalDependencyRow).join("")}</div>
            </details>`
          : ""
      }
      ${renderDependencyInstallOrder(plan)}
    </div>
  `;
}

function renderDependencyInstallOrder(plan: DependencyPlan) {
  const order = plan.install_order?.filter(Boolean) ?? [];
  if (order.length <= 1) return "";
  return `<p class="dependency-install-order">Install order: ${escapeHtml(order.join(" → "))}</p>`;
}

function renderRequiredDependencyRow(dependency: DependencyPlan["required_dependencies"][number]) {
  return renderDependencyRow(dependency, dependencyStatusText(dependency), dependencyDetailText(dependency));
}

function renderOptionalDependencyRow(dependency: DependencyPlan["optional_dependencies"][number]) {
  return renderDependencyRow(dependency, optionalDependencyStatusText(dependency), dependencyDetailText(dependency));
}

function renderDependencyRow(dependency: DependencyPlan["required_dependencies"][number], status: string, detail: string) {
  const constraint = dependency.constraint ? ` ${dependency.constraint}` : "";
  const relation = dependencyRelationText(dependency);
  const detailText = detail ? `${relation} · ${detail}` : relation;
  return `
    <div class="dependency-row ${dependencyStatusClass(dependency.status)}" style="--dependency-depth: ${dependencyDepthOffset(dependency)}">
      <strong>${escapeHtml(dependency.name)}${escapeHtml(constraint)}</strong>
      <span>${escapeHtml(status)}</span>
      <span>${escapeHtml(detailText)}</span>
    </div>
  `;
}

function requiredDependencySummary(plan: DependencyPlan) {
  if (hasRequiredDependencyIssues(plan)) return "Blocked";
  const willInstall = plan.required_dependencies.filter((dependency) => dependency.status === "will-install").length;
  const installed = plan.required_dependencies.filter((dependency) => dependency.status === "already-installed").length;
  if (willInstall > 0) return `${willInstall} will install`;
  if (installed > 0) return "Already satisfied";
  return "None";
}

function dependencyStatusText(dependency: DependencyPlan["required_dependencies"][number]) {
  if (dependency.status === "already-installed") return "Already installed";
  if (dependency.status === "will-install") return dependency.bundled_folder ? "Bundled" : "Will install";
  if (dependency.status === "not-installed") return "Not installed";
  if (dependency.status === "ambiguous") return "Ambiguous";
  if (dependency.status === "unresolved") return "Unresolved";
  if (dependency.status === "circular") return "Circular";
  if (dependency.status === "max-depth") return "Max depth";
  return dependency.status;
}

function optionalDependencyStatusText(dependency: DependencyPlan["optional_dependencies"][number]) {
  if (dependency.status === "already-installed") return "Already installed";
  if (dependency.status === "not-installed") return "Not installed";
  if (dependency.status === "unresolved") return "Not resolved";
  if (dependency.status === "ambiguous") return "Ambiguous";
  if (dependency.status === "will-install") return dependency.bundled_folder ? "Bundled" : "Will install";
  if (dependency.status === "circular") return "Circular";
  if (dependency.status === "max-depth") return "Max depth";
  return dependency.status;
}

function dependencyDetailText(dependency: DependencyPlan["required_dependencies"][number]) {
  if (dependency.installed_folder) return dependency.installed_folder;
  if (dependency.bundled_folder) return dependency.bundled_folder;
  if (dependency.remote_name) return dependency.remote_name;
  if (dependency.status === "ambiguous") return "Multiple remote matches";
  if (dependency.status === "circular") return "Circular dependency";
  if (dependency.status === "max-depth") return "Max depth reached";
  if (dependency.status === "unresolved") return "No safe remote match";
  return "Not installed automatically";
}

function dependencyStatusClass(status: string) {
  if (status === "already-installed") return "is-installed";
  if (status === "will-install") return "is-planned";
  if (status === "ambiguous" || status === "unresolved" || status === "circular" || status === "max-depth") return "is-warning";
  return "is-muted";
}

function isSafeNewInstallPlan(plan: PlanRemoteInstallResponse) {
  return (
    plan.plan.items.length > 0 &&
    plan.plan.items.every((item) => item.action === "would-install-new") &&
    isSafeDependencyPlan(plan.dependency_plan)
  );
}

function hasReplacementPlanItems(plan: PlanRemoteInstallResponse) {
  return plan.plan.items.some((item) => item.action === "would-replace-existing") || hasDependencyReplacementItems(plan.dependency_plan);
}

function hasInstallablePlanItems(plan: PlanRemoteInstallResponse) {
  return plan.plan.items.some((item) => item.action === "would-install-new" || item.action === "would-replace-existing");
}

function hasSkippedPlanItems(plan: PlanRemoteInstallResponse) {
  return (
    plan.plan.items.some((item) => item.action !== "would-install-new" && item.action !== "would-replace-existing") ||
    plan.dependency_plan.install_items.some(
      (item) => item.role === "required-dependency" && item.action !== "would-install-new" && item.action !== "would-replace-existing",
    )
  );
}

function isSafeDependencyPlan(plan: DependencyPlan) {
  return (
    !hasRequiredDependencyIssues(plan) &&
    plan.install_items
      .filter((item) => item.role === "required-dependency")
      .every((item) => item.action === "would-install-new")
  );
}

function hasRequiredDependencyIssues(plan: DependencyPlan | null) {
  return Boolean(
    plan?.required_dependencies.some(
      (dependency) =>
        dependency.status === "unresolved" ||
        dependency.status === "ambiguous" ||
        dependency.status === "circular" ||
        dependency.status === "max-depth",
    ),
  );
}

function hasDependencyReplacementItems(plan: DependencyPlan | null) {
  return Boolean(plan?.install_items.some((item) => item.role === "required-dependency" && item.action === "would-replace-existing"));
}

function installDependencyConfirmText(plan: DependencyPlan) {
  const unresolved = plan.required_dependencies.filter(
    (dependency) =>
      dependency.status === "unresolved" ||
      dependency.status === "ambiguous" ||
      dependency.status === "circular" ||
      dependency.status === "max-depth",
  );
  if (unresolved.length > 0) {
    return `Some required dependencies could not be resolved: ${unresolved.map((dependency) => dependency.name).join(", ")}.`;
  }
  const willInstall = plan.required_dependencies.filter((dependency) => dependency.status === "will-install").length;
  if (willInstall > 0) {
    return `The app will also install ${willInstall} required ${willInstall === 1 ? "library" : "libraries"}.`;
  }
  return "No required library changes are currently expected.";
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
  bindAddonContextMenuEvents();
  document.querySelectorAll<HTMLButtonElement>("[data-tab]").forEach((button) => {
    button.addEventListener("click", () => {
      state.tab = button.dataset.tab as Tab;
      state.error = null;
      clearSuccess();
      state.warning = null;
      closeAddonContextMenu(false);
      render();
      if (state.tab === "search") {
        ensureSearchLoaded();
      }
    });
  });
}

function bindInitialSetupEvents() {
  document.querySelector<HTMLInputElement>(`#${TEXT_INPUT_IDS.setupAddonsPath}`)?.addEventListener("input", (event) => {
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

function bindAddonContextMenuEvents() {
  document.querySelectorAll<HTMLButtonElement>("[data-addon-context-action]").forEach((button) => {
    button.addEventListener("click", () => {
      runAddonContextAction(button.dataset.addonContextAction as AddonContextAction);
    });
  });
}

function handleCardClick(card: HTMLElement) {
  closeAddonContextMenu(false);
  if (card.dataset.addonId) {
    loadDetails(card.dataset.addonId);
    return;
  }
  if (card.dataset.installedFolder) openInstalledDetails(card.dataset.installedFolder);
}

function bindTabEvents() {
  document.querySelector<HTMLButtonElement>("#refresh-installed")?.addEventListener("click", () => loadInstalled(true));
  document.querySelector<HTMLButtonElement>("#apply-update-all-installed")?.addEventListener("click", applyUpdateAll);
  document.querySelector<HTMLButtonElement>("#open-settings")?.addEventListener("click", () => {
    state.tab = "settings";
    state.activeSettingsSection = "folders";
    render();
  });
  document.querySelector<HTMLInputElement>(`#${TEXT_INPUT_IDS.installedFilter}`)?.addEventListener("input", (event) => {
    state.installedQuery = (event.currentTarget as HTMLInputElement).value;
    renderInstalledListOnly();
  });
  document.querySelector<HTMLSelectElement>("#installed-sort")?.addEventListener("change", (event) => {
    state.installedSort = (event.currentTarget as HTMLSelectElement).value as InstalledSort;
    render();
  });
  document.querySelector<HTMLInputElement>(`#${TEXT_INPUT_IDS.addonSearch}`)?.addEventListener("input", (event) => {
    const value = (event.currentTarget as HTMLInputElement).value;
    state.searchQuery = value;
  });
  document.querySelector<HTMLInputElement>(`#${TEXT_INPUT_IDS.addonSearch}`)?.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      runSearch();
    }
  });
  document.querySelector<HTMLSelectElement>("#search-limit")?.addEventListener("change", (event) => {
    state.searchLimit = Number((event.currentTarget as HTMLSelectElement).value);
    resetSearchPagination();
    void loadSearchResults();
  });
  document.querySelector<HTMLSelectElement>("#search-category")?.addEventListener("change", (event) => {
    state.searchCategoryId = (event.currentTarget as HTMLSelectElement).value;
    resetSearchPagination();
    void loadSearchResults();
  });
  document.querySelectorAll<HTMLButtonElement>("[data-search-mode]").forEach((button) => {
    button.addEventListener("click", () => {
      state.searchMode = button.dataset.searchMode as SearchMode;
      resetSearchPagination();
      void loadSearchResults();
    });
  });
  document.querySelector<HTMLButtonElement>("#run-search")?.addEventListener("click", runSearch);
  document.querySelectorAll<HTMLElement>(".clickable").forEach((card) => {
    card.addEventListener("click", () => handleCardClick(card));
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
  document.querySelectorAll<HTMLButtonElement>("[data-open-installed-dependency]").forEach((button) => {
    button.addEventListener("click", () => openInstalledDetails(button.dataset.openInstalledDependency ?? ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-open-remote-dependency]").forEach((button) => {
    button.addEventListener("click", () => loadDetails(button.dataset.openRemoteDependency ?? ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-install-dependency]").forEach((button) => {
    button.addEventListener("click", () => installDependency(button.dataset.installDependency ?? ""));
  });
  document.querySelectorAll<HTMLButtonElement>("[data-resolve-folder]").forEach((button) => {
    button.addEventListener("click", () => openResolveRemoteMatch(button.dataset.resolveFolder ?? ""));
  });
  document.querySelectorAll<HTMLElement>("[data-resolve-candidate]").forEach((card) => {
    card.addEventListener("click", () => selectResolveCandidate(card.dataset.resolveCandidate ?? ""));
    card.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" && event.key !== " ") return;
      event.preventDefault();
      selectResolveCandidate(card.dataset.resolveCandidate ?? "");
    });
  });
  document.querySelector<HTMLButtonElement>("#cancel-resolve-remote-match")?.addEventListener("click", closeResolveRemoteMatch);
  document.querySelector<HTMLButtonElement>("#link-resolve-candidate")?.addEventListener("click", linkResolveCandidate);
  document.querySelector<HTMLButtonElement>("#reinstall-resolve-candidate")?.addEventListener("click", reinstallResolveCandidate);
  document.querySelectorAll<HTMLButtonElement>("[data-resolve-website]").forEach((button) => {
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      const url = button.dataset.resolveWebsite ?? "";
      if (url) void openExternalUrl(url);
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
  document.querySelector<HTMLInputElement>("#remove-addon-savedvariables")?.addEventListener("change", (event) => {
    state.removeSavedVariables = (event.currentTarget as HTMLInputElement).checked;
  });
  document.querySelector<HTMLButtonElement>("#cancel-clear-savedvariables")?.addEventListener("click", cancelClearSavedVariables);
  document.querySelector<HTMLButtonElement>("#confirm-clear-savedvariables")?.addEventListener("click", confirmClearSavedVariables);
  document.querySelector<HTMLButtonElement>("#create-manual-backup")?.addEventListener("click", requestManualBackup);
  document.querySelector<HTMLButtonElement>("#restore-backup")?.addEventListener("click", requestRestoreBackup);
  document.querySelector<HTMLButtonElement>("#cancel-manual-backup")?.addEventListener("click", cancelManualBackup);
  document.querySelector<HTMLButtonElement>("#confirm-manual-backup")?.addEventListener("click", confirmManualBackup);
  document.querySelector<HTMLInputElement>("#manual-backup-savedvariables")?.addEventListener("change", (event) => {
    state.manualBackupIncludeSavedVariables = (event.currentTarget as HTMLInputElement).checked;
  });
  document.querySelector<HTMLButtonElement>("#cancel-restore-backup")?.addEventListener("click", cancelRestoreBackup);
  document.querySelector<HTMLButtonElement>("#confirm-restore-backup")?.addEventListener("click", confirmRestoreBackup);
  document.querySelector<HTMLInputElement>("#restore-backup-addons")?.addEventListener("change", (event) => {
    state.restoreAddons = (event.currentTarget as HTMLInputElement).checked;
  });
  document.querySelector<HTMLInputElement>("#restore-backup-savedvariables")?.addEventListener("change", (event) => {
    state.restoreSavedVariables = (event.currentTarget as HTMLInputElement).checked;
  });
  document.querySelector<HTMLButtonElement>("#open-created-backup-folder")?.addEventListener("click", openCreatedBackupFolder);
  document.querySelector<HTMLButtonElement>("#refresh-installed-after-install")?.addEventListener("click", () => loadInstalled(true));
  document.querySelectorAll<HTMLButtonElement>("[data-apply-update-target]").forEach((button) => {
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      applySingleUpdate(button.dataset.applyUpdateTarget ?? "");
    });
  });
  document.querySelector<HTMLButtonElement>("#save-settings")?.addEventListener("click", saveSettings);
  document.querySelector<HTMLButtonElement>("#reset-settings")?.addEventListener("click", resetSettings);
  document.querySelector<HTMLButtonElement>("#clear-http-cache")?.addEventListener("click", clearHttpCache);
  document.querySelectorAll<HTMLButtonElement>("[data-settings-section]").forEach((button) => {
    button.addEventListener("click", () => {
      syncSettingsDraft();
      state.activeSettingsSection = button.dataset.settingsSection as SettingsSection;
      render();
    });
  });
  document.querySelector<HTMLInputElement>(`#${TEXT_INPUT_IDS.settingsAddonsPath}`)?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>(`#${TEXT_INPUT_IDS.settingsBackupFolder}`)?.addEventListener("input", () => {
    const hadBackupFolder = Boolean(state.settings?.backup_dir_override);
    syncSettingsDraft();
    if (hadBackupFolder !== Boolean(state.settings?.backup_dir_override)) render();
  });
  document.querySelector<HTMLInputElement>(`#${TEXT_INPUT_IDS.settingsDownloadFolder}`)?.addEventListener("input", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-keep-downloads")?.addEventListener("change", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-include-unknown")?.addEventListener("change", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-hide-libraries-search")?.addEventListener("change", syncSettingsDraft);
  document.querySelector<HTMLInputElement>("#settings-hide-libraries-installed")?.addEventListener("change", syncSettingsDraft);
  document.querySelectorAll<HTMLButtonElement>("[data-browse-target]").forEach((button) => {
    button.addEventListener("click", () => browseSettingsFolder(button.dataset.browseTarget ?? ""));
  });
  if (state.tab === "search") {
    ensureSearchLoaded();
    bindSearchScrollLoading();
  }
  if (state.tab === "settings" && state.activeSettingsSection === "cache") {
    ensureHttpCacheStatsLoaded();
  }
}

function singleUpdateNoChangeMessage(result: SingleUpdateApplyResponse) {
  if (result.decision === "skipped-current") return "Already current.";
  if (result.decision === "skipped-ambiguous") return "Remote match is ambiguous.";
  if (result.decision === "skipped-no-match") return "No clean remote match is available.";
  if (result.decision === "skipped-local-newer") return "The local version appears newer than remote.";
  if (result.decision === "skipped-unknown-use-force") return "Version check is uncertain.";
  return result.reason ?? "No addon folders were updated.";
}

function singleUpdateSuccessMessage(result: SingleUpdateApplyResponse) {
  if (!result.applied) return singleUpdateNoChangeMessage(result);
  return `Updated ${plainEsoText(result.remote_details?.name ?? result.remote?.name ?? result.local.folder_name)}.`;
}

function updateAllSuccessMessage(result: ApplyUpdateAllResponse) {
  const count = result.results.length;
  if (count === 0) return "Already current.";
  return `Updated ${count} addon${count === 1 ? "" : "s"}.`;
}

function preventProductionDevToolsShortcut(event: KeyboardEvent) {
  if (!disableDevToolsShortcuts || !isDevToolsShortcut(event)) return;
  event.preventDefault();
  event.stopImmediatePropagation();
}

function isDevToolsShortcut(event: KeyboardEvent) {
  const key = event.key.toLowerCase();
  const ctrlOrMeta = event.ctrlKey || event.metaKey;
  const macInspectorChord = event.metaKey && event.altKey && ["i", "j", "c"].includes(key);
  const inspectorChord = ctrlOrMeta && event.shiftKey && ["i", "j", "c"].includes(key);
  return event.key === "F12" || macInspectorChord || inspectorChord;
}

function handleGlobalContextMenu(event: MouseEvent) {
  event.preventDefault();

  const target = event.target;
  if (!(target instanceof Element)) {
    closeAddonContextMenu();
    return;
  }

  if (target.closest(".addon-context-menu")) return;

  if (target.closest("button, a, input, select, textarea, [contenteditable='true']")) {
    closeAddonContextMenu();
    return;
  }

  const card = target.closest<HTMLElement>("[data-addon-context-menu='true'][data-installed-folder]");
  if (!card?.dataset.installedFolder) {
    closeAddonContextMenu();
    return;
  }

  event.stopPropagation();
  showAddonContextMenu(card.dataset.installedFolder, event.clientX, event.clientY);
}

function showAddonContextMenu(folderName: string, x: number, y: number) {
  const local = installedLocalByFolder(folderName);
  if (!local) {
    closeAddonContextMenu();
    return;
  }

  const position = clampedContextMenuPosition(x, y);
  state.addonContextMenu = {
    folderName: local.folder_name,
    x: position.x,
    y: position.y,
  };
  render();
}

function closeAddonContextMenu(renderNow = true) {
  if (!state.addonContextMenu) return;
  state.addonContextMenu = null;
  if (renderNow) render();
}

function clampedContextMenuPosition(x: number, y: number) {
  const maxX = Math.max(CONTEXT_MENU_MARGIN, window.innerWidth - ADDON_CONTEXT_MENU_WIDTH - CONTEXT_MENU_MARGIN);
  const maxY = Math.max(CONTEXT_MENU_MARGIN, window.innerHeight - ADDON_CONTEXT_MENU_HEIGHT - CONTEXT_MENU_MARGIN);
  return {
    x: Math.min(Math.max(x, CONTEXT_MENU_MARGIN), maxX),
    y: Math.min(Math.max(y, CONTEXT_MENU_MARGIN), maxY),
  };
}

function runAddonContextAction(action: AddonContextAction) {
  const menu = state.addonContextMenu;
  if (!menu) return;
  const local = installedLocalByFolder(menu.folderName);
  state.addonContextMenu = null;

  if (!local) {
    state.error = "Addon is no longer installed.";
    render();
    return;
  }

  if (action === "uninstall") {
    requestUninstallAddon(local);
    return;
  }
  if (action === "clear-savedvariables") {
    requestClearSavedVariables(local);
    return;
  }
  void openAddonFolder(local);
}

function setSuccess(message: string | null, detail: string | null = null) {
  state.success = message;
  state.successDetail = message ? detail : null;
}

function clearSuccess() {
  setSuccess(null);
}

async function withLoading(task: () => Promise<void>, operation: OperationKind = "general", operationTarget: string | null = null) {
  state.loading = true;
  state.operation = operation;
  state.operationTarget = operationTarget;
  state.error = null;
  clearSuccess();
  if (operation === "manual-backup" || operation === "backup-restore") {
    state.manualBackupError = null;
  }
  closeAddonContextMenu(false);
  render();
  try {
    await task();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    if (operation === "manual-backup" || operation === "backup-restore") {
      state.manualBackupConfirmOpen = false;
      state.restoreInspection = null;
      state.restoreZipPath = null;
      state.manualBackupError = message;
    } else {
      state.error = operationErrorMessage(operation, operationTarget, message);
    }
  } finally {
    state.loading = false;
    state.operation = null;
    state.operationTarget = null;
    render();
  }
}

function operationErrorMessage(operation: OperationKind, operationTarget: string | null, message: string) {
  if (operation === "update-apply") {
    return operationTarget ? `Could not update ${operationTarget}. ${message}` : `Could not apply update. ${message}`;
  }
  if (operation === "update-all-apply") {
    return `Could not apply Update All. ${message}`;
  }
  if (operation === "resolve-search") {
    return operationTarget ? `Could not resolve ${operationTarget}. ${message}` : `Could not resolve remote match. ${message}`;
  }
  if (operation === "resolve-link") {
    return operationTarget ? `Could not link ${operationTarget}. ${message}` : `Could not link remote match. ${message}`;
  }
  if (operation === "resolve-reinstall") {
    return operationTarget ? `Could not reinstall ${operationTarget}. ${message}` : `Could not reinstall selected addon. ${message}`;
  }
  return message;
}

function invokeWithTimeout<T>(command: string, args: Record<string, unknown>, timeoutMs: number, timeoutMessage: string): Promise<T> {
  let timeoutId: number | undefined;
  const timeout = new Promise<never>((_, reject) => {
    timeoutId = window.setTimeout(() => reject(new Error(timeoutMessage)), timeoutMs);
  });

  return Promise.race([invoke<T>(command, args), timeout]).finally(() => {
    if (timeoutId !== undefined) window.clearTimeout(timeoutId);
  });
}

function removeSuccessMessage(result: RemoveInstalledAddonResponse) {
  if (result.removed_addon && !result.removed_saved_variables) return "Addon uninstalled. SavedVariables were kept.";
  if (result.removed_addon && result.removed_saved_variables) return "Addon and SavedVariables removed.";
  return result.message;
}

function removeSavedVariablesStatusText(result: RemoveInstalledAddonResponse) {
  if (!result.removed_saved_variables) return null;
  if (result.saved_variables_deleted_count === 0) return "No SavedVariables files were found.";
  return `Deleted ${formatCount(result.saved_variables_deleted_count)} SavedVariables ${result.saved_variables_deleted_count === 1 ? "file" : "files"}.`;
}

function nextFrame() {
  return new Promise<void>((resolve) => window.requestAnimationFrame(() => resolve()));
}

function renderInstalledListOnly() {
  const list = document.querySelector<HTMLElement>("#installed-list");
  if (!list) return;
  list.innerHTML = renderInstalledList();
  bindCardEventsOnly();
}

function bindCardEventsOnly() {
  document.querySelectorAll<HTMLElement>(".clickable").forEach((card) => {
    card.addEventListener("click", () => handleCardClick(card));
  });
  document.querySelectorAll<HTMLElement>(".addon-card button, .addon-card a").forEach((element) => {
    element.addEventListener("click", (event) => event.stopPropagation());
  });
  document.querySelectorAll<HTMLButtonElement>("[data-apply-update-target]").forEach((button) => {
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      applySingleUpdate(button.dataset.applyUpdateTarget ?? "");
    });
  });
  document.querySelectorAll<HTMLButtonElement>("[data-resolve-folder]").forEach((button) => {
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      openResolveRemoteMatch(button.dataset.resolveFolder ?? "");
    });
  });
}

function renderSearchResultsOnly() {
  const summary = document.querySelector<HTMLElement>("#search-result-summary");
  if (summary) {
    summary.outerHTML = renderSearchResultSummary(false);
  }

  const list = document.querySelector<HTMLElement>("#search-list");
  if (!list) return;
  list.innerHTML = renderSearchList(false);

  const status = document.querySelector<HTMLElement>("#search-load-status");
  const statusHtml = renderSearchIncrementStatus(false);
  if (status) {
    if (statusHtml) {
      status.outerHTML = statusHtml;
    } else {
      status.remove();
    }
  } else if (statusHtml) {
    list.insertAdjacentHTML("afterend", statusHtml);
  }

  bindCardEventsOnly();
  bindSearchScrollLoading();
}

function bindSearchScrollLoading() {
  unbindSearchScrollListener();
  if (state.tab !== "search" || !state.searchLoaded || !hasMoreSearchResults() || isSearchLoading()) return;

  const container = document.querySelector<HTMLElement>(".content");
  if (!container) return;

  const handler = () => {
    if (state.tab !== "search" || !state.searchLoaded || !hasMoreSearchResults() || isSearchLoading()) return;
    const distanceFromBottom = container.scrollHeight - container.scrollTop - container.clientHeight;
    if (distanceFromBottom > SEARCH_SCROLL_THRESHOLD_PX) return;
    loadMoreSearchResults();
  };

  container.addEventListener("scroll", handler, { passive: true });
  searchScrollContainer = container;
  searchScrollHandler = handler;
}

function unbindSearchScrollListener() {
  if (searchScrollContainer && searchScrollHandler) {
    searchScrollContainer.removeEventListener("scroll", searchScrollHandler);
  }
  searchScrollContainer = null;
  searchScrollHandler = null;
}

function loadMoreSearchResults() {
  if (!hasMoreSearchResults()) return;
  const nextVisibleCount = Math.min(state.visibleSearchCount + state.searchPageSize, totalVisibleSearchMatches());
  if (nextVisibleCount === state.visibleSearchCount) return;
  state.visibleSearchCount = nextVisibleCount;
  renderSearchResultsOnly();
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

async function browseForBackupZip(defaultPath: string | null | undefined) {
  try {
    const selected = await openDialog({
      directory: false,
      multiple: false,
      defaultPath: defaultPath || undefined,
      filters: [{ name: "ZIP backups", extensions: ["zip"] }],
    });

    return typeof selected === "string" ? selected : null;
  } catch (error) {
    state.manualBackupError = `Could not open backup picker. ${error instanceof Error ? error.message : String(error)}`;
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
  resetSearchPagination();
  return loadSearchResults(true);
}

function ensureSearchLoaded() {
  if (state.searchLoaded || state.searchLoadAttempted || state.operation === "search") return;
  void loadSearchResults();
}

function loadSearchResults(refresh = false) {
  state.searchLoadAttempted = true;
  resetSearchPagination();
  return withLoading(async () => {
    const response = await invoke<BrowseRemoteAddonsResponse>("browse_remote_addons", {
      mode: state.searchMode,
      categoryId: state.searchCategoryId || null,
      query: state.searchAppliedQuery || null,
      limit: state.searchLimit,
      path: effectiveAddonsPath(),
      refresh,
      hideLibraries: state.settings?.hide_libraries_in_search ?? false,
    });
    state.searchMode = response.mode === "recent" ? "recent" : "most_downloaded";
    state.searchAppliedQuery = response.query;
    state.searchCategoryId = response.category_id ?? "";
    if (!state.searchAppliedQuery) {
      state.searchLimit = response.limit;
    }
    state.searchResults = response.results;
    state.totalSearchMatches = response.results.length;
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
  state.selectedDependencies = null;
  state.selectedDependenciesLoading = false;
  state.selectedDependenciesError = null;
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
      state.selectedDependencies = null;
      state.selectedDependenciesError = null;
      state.selectedDependenciesLoading = Boolean(response.local);
      state.warning = [response.cache_warning, response.local_warning].filter(Boolean).join(" ") || null;
      if (response.local) void loadInstalledDependencies(response.local.folder_name);
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
  state.selectedDependencies = null;
  state.selectedDependenciesLoading = true;
  state.selectedDependenciesError = null;
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  const uid = match?.remote?.uid;
  if (uid) {
    return withLoading(async () => {
      const [details, dependencies] = await Promise.all([
        invoke<AddonDetails>("get_remote_addon_details", { addonId: uid }),
        fetchInstalledDependencies(folderName),
      ]);
      if (state.selectedLocal?.folder_name === folderName) {
        state.selectedDetails = details;
        if (dependencies) {
          state.selectedDependencies = dependencies;
          state.selectedDependenciesError = null;
        }
        state.selectedDependenciesLoading = false;
        void cacheSelectedImages();
      }
    }, "details");
  } else {
    render();
    void loadInstalledDependencies(folderName);
  }
}

function openResolveRemoteMatch(folderName: string) {
  const addon = installedLocalByFolder(folderName);
  if (!addon) {
    state.error = "Addon is no longer installed.";
    render();
    return;
  }

  state.resolveLocal = addon;
  state.resolveCandidates = [];
  state.resolveSelectedUid = null;
  state.resolveMessage = null;
  state.removeResult = null;
  closeAddonContextMenu(false);

  return withLoading(async () => {
    const response = await invoke<RemoteMatchCandidatesResponse>("find_remote_match_candidates", {
      localFolder: folderName,
      path: effectiveAddonsPath(),
    });
    if (state.resolveLocal?.folder_name.toLowerCase() !== folderName.toLowerCase()) return;
    state.resolveLocal = response.local;
    state.resolveCandidates = response.candidates;
    state.resolveMessage = response.message;
    state.resolveSelectedUid = defaultResolveSelectedUid(response.candidates);
  }, "resolve-search", folderName);
}

function defaultResolveSelectedUid(candidates: RemoteMatchCandidate[]) {
  if (candidates.length === 1 && candidates[0].confidence === "very-high") {
    return candidates[0].remote_uid;
  }
  return null;
}

function selectResolveCandidate(remoteUid: string) {
  if (state.loading || !remoteUid) return;
  state.resolveSelectedUid = remoteUid;
  render();
}

function closeResolveRemoteMatch() {
  if (guardedOperationRunning()) return;
  clearResolveRemoteMatch();
  render();
}

function clearResolveRemoteMatch() {
  state.resolveLocal = null;
  state.resolveCandidates = [];
  state.resolveSelectedUid = null;
  state.resolveMessage = null;
}

async function linkResolveCandidate() {
  const local = state.resolveLocal;
  const selected = selectedResolveCandidate();
  if (!local || !selected) return;

  await withLoading(async () => {
    const result = await invoke<LinkInstalledAddonToRemoteResponse>("link_installed_addon_to_remote", {
      localFolder: local.folder_name,
      remoteUid: selected.remote_uid,
      path: effectiveAddonsPath(),
    });
    state.path = result.addons_dir;
    await refreshInstalledData(true);
    clearResolveRemoteMatch();
    setSuccess(`Linked ${plainEsoText(local.title ?? local.folder_name)} to ${plainEsoText(result.remote_name ?? selected.remote_name ?? selected.remote_uid)}.`);
  }, "resolve-link", local.folder_name);
}

async function reinstallResolveCandidate() {
  const local = state.resolveLocal;
  const selected = selectedResolveCandidate();
  if (!local || !selected) return;

  const confirmed = window.confirm(
    `Reinstall ${selected.remote_name ?? selected.remote_uid} for ${local.folder_name}?\n\nThis will download fresh ESOUI metadata, verify the ZIP MD5, validate the ZIP, and replace only the selected addon folder if the package targets it. SavedVariables will not be touched.`,
  );
  if (!confirmed) return;

  await withLoading(async () => {
    const result = await invoke<InstallRemoteAddonResponse>("reinstall_installed_addon_from_remote", {
      localFolder: local.folder_name,
      remoteUid: selected.remote_uid,
      path: effectiveAddonsPath(),
      backupDir: state.settings?.backup_dir_override || null,
      keepDownload: state.settings?.keep_downloads_default ?? false,
      downloadDir: state.settings?.download_dir || null,
    });
    state.path = result.addons_dir;
    await refreshInstalledData(true);
    clearResolveRemoteMatch();
    setSuccess(`Reinstalled ${plainEsoText(result.remote.name ?? selected.remote_name ?? selected.remote_uid)}.`);
  }, "resolve-reinstall", local.folder_name);
}

function installedLocalByFolder(folderName: string) {
  return state.installed?.addons.find((addon) => addon.folder_name.toLowerCase() === folderName.toLowerCase()) ?? null;
}

async function fetchInstalledDependencies(folderName: string) {
  try {
    return await invoke<InstalledAddonDependenciesResponse>("get_installed_addon_dependencies", {
      folderName,
      path: effectiveAddonsPath(),
    });
  } catch (error) {
    if (state.selectedLocal?.folder_name === folderName) {
      state.selectedDependenciesError = `Dependency details could not be loaded. ${error instanceof Error ? error.message : String(error)}`;
    }
    return null;
  }
}

async function loadInstalledDependencies(folderName: string) {
  state.selectedDependenciesLoading = true;
  const dependencies = await fetchInstalledDependencies(folderName);
  if (state.selectedLocal?.folder_name !== folderName) return;
  state.selectedDependencies = dependencies;
  state.selectedDependenciesLoading = false;
  if (dependencies) state.selectedDependenciesError = null;
  render();
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
  state.selectedDependencies = null;
  state.selectedDependenciesLoading = false;
  state.selectedDependenciesError = null;
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
  state.clearSavedVariablesConfirmLocal = null;
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
  if (hasRequiredDependencyIssues(plan.dependency_plan)) {
    state.error = "Some required dependencies could not be resolved safely. Nothing was installed.";
    render();
    return;
  }
  const backupText = plan.plan.items.some((item) => item.action === "would-replace-existing")
    ? "Existing addon folders may be backed up and replaced."
    : "No existing addon folder replacement is currently expected.";
  const dependencyText = installDependencyConfirmText(plan.dependency_plan);
  const confirmed = window.confirm(
    `Install ${plan.remote.name ?? addonId}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n${dependencyText}\n\nThe app will fetch fresh metadata, download and verify the ZIP, validate it, build a fresh preview, and back up replacements before applying.`,
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

function installDependency(remoteUid: string) {
  if (!remoteUid) return;
  const parentFolder = state.selectedLocal?.folder_name ?? null;
  return withLoading(async () => {
    const plan = await invoke<PlanRemoteInstallResponse>("plan_remote_install", {
      addonId: remoteUid,
      path: effectiveAddonsPath(),
    });

    if (!hasInstallablePlanItems(plan)) {
      throw new Error("No valid addon folders were found in this dependency package. Nothing was installed.");
    }
    if (hasRequiredDependencyIssues(plan.dependency_plan)) {
      throw new Error("Some required dependencies could not be resolved safely. Nothing was installed.");
    }

    let result: InstallRemoteAddonResponse | null = null;
    if (isSafeNewInstallPlan(plan)) {
      result = await invoke<InstallRemoteAddonResponse>("install_remote_addon_new_only", {
        addonId: remoteUid,
        path: effectiveAddonsPath(),
        backupDir: state.settings?.backup_dir_override || null,
        keepDownload: state.settings?.keep_downloads_default ?? false,
        downloadDir: state.settings?.download_dir || null,
      });
    } else {
      const backupText = plan.plan.items.some((item) => item.action === "would-replace-existing")
        ? "Existing addon folders may be backed up and replaced."
        : "No existing addon folder replacement is currently expected.";
      const dependencyText = installDependencyConfirmText(plan.dependency_plan);
      const confirmed = window.confirm(
        `Install dependency ${plan.remote.name ?? remoteUid}?\n\nTarget AddOns directory:\n${plan.addons_dir}\n\n${backupText}\n${dependencyText}\n\nThe app will fetch fresh metadata, download and verify the ZIP, validate it, build a fresh preview, and back up replacements before applying.`,
      );
      if (!confirmed) return;
      result = await invoke<InstallRemoteAddonResponse>("install_remote_addon", {
        addonId: remoteUid,
        path: effectiveAddonsPath(),
        backupDir: state.settings?.backup_dir_override || null,
        keepDownload: state.settings?.keep_downloads_default ?? false,
        downloadDir: state.settings?.download_dir || null,
      });
    }

    if (!result) return;
    state.path = result.addons_dir;
    setSuccess(`Installed dependency ${result.remote.name ?? plan.remote.name ?? remoteUid}.`);
    await refreshInstalledData(true);
    if (parentFolder && state.selectedLocal?.folder_name === parentFolder) {
      state.selectedLocal = installedLocalByFolder(parentFolder) ?? state.selectedLocal;
      state.selectedDependenciesLoading = true;
      const dependencies = await fetchInstalledDependencies(parentFolder);
      if (state.selectedLocal?.folder_name === parentFolder) {
        state.selectedDependencies = dependencies;
        state.selectedDependenciesLoading = false;
        if (dependencies) state.selectedDependenciesError = null;
      }
    }
  }, "dependency-install", remoteUid);
}

async function applySingleUpdate(target: string) {
  if (!target) return;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  state.updateAllPlan = null;
  state.updateAllResult = null;
  state.loading = true;
  state.operation = "update-apply";
  state.operationTarget = target;
  state.singleUpdatePhase = "preparing";
  state.error = null;
  clearSuccess();
  closeAddonContextMenu(false);
  render();

  try {
    await nextFrame();
    state.singleUpdatePhase = "updating";
    render();
    state.singleUpdateResult = await invoke<SingleUpdateApplyResponse>("apply_single_update", {
      target,
      path: effectiveAddonsPath(),
      backupDir: state.settings?.backup_dir_override || null,
      keepDownload: state.settings?.keep_downloads_default ?? false,
      downloadDir: state.settings?.download_dir || null,
      force: state.forceUpdate,
    });
    state.path = state.singleUpdateResult.addons_dir;
    state.updateAllPlan = null;
    state.updateAllResult = null;
    setSuccess(singleUpdateSuccessMessage(state.singleUpdateResult));
    await refreshInstalledData(true);
    syncSelectedStateAfterUpdate(target);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    state.error = operationErrorMessage("update-apply", target, message);
  } finally {
    state.loading = false;
    state.operation = null;
    state.operationTarget = null;
    state.singleUpdatePhase = null;
    render();
  }
}

function removeAddon() {
  const local = state.selectedLocal;
  if (!local) return;
  requestUninstallAddon(local);
}

function requestUninstallAddon(local: LocalAddon) {
  state.removeConfirmLocal = local;
  state.removeSavedVariables = false;
  state.clearSavedVariablesConfirmLocal = null;
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
  const removeSavedVariables = state.removeSavedVariables;
  state.installPlan = null;
  state.installResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  return withLoading(async () => {
    state.removeResult = await invoke<RemoveInstalledAddonResponse>("remove_installed_addon", {
      folderName: local.folder_name,
      path: effectiveAddonsPath(),
      removeSavedVariables,
    });
    state.removeConfirmLocal = null;
    state.removeSavedVariables = false;
    setSuccess(removeSuccessMessage(state.removeResult), removeSavedVariablesStatusText(state.removeResult));
    state.installed = await invoke<InstalledAddonsResponse>("get_installed_addons", { path: effectiveAddonsPath() });
    state.path = state.installed.addons_dir;
    syncInstalledStateAfterRemove(state.removeResult);
  }, "remove-apply");
}

function requestClearSavedVariables(local: LocalAddon) {
  state.clearSavedVariablesConfirmLocal = local;
  state.removeConfirmLocal = null;
  render();
}

function cancelClearSavedVariables() {
  if (guardedOperationRunning()) return;
  state.clearSavedVariablesConfirmLocal = null;
  render();
}

function confirmClearSavedVariables() {
  const local = state.clearSavedVariablesConfirmLocal;
  if (!local) return;
  state.clearSavedVariablesResult = null;
  return withLoading(async () => {
    const result = await invoke<ClearSavedVariablesResponse>("clear_saved_variables", {
      folderName: local.folder_name,
      path: effectiveAddonsPath(),
    });
    state.clearSavedVariablesResult = result;
    state.clearSavedVariablesConfirmLocal = null;
    setSuccess(result.deleted_count > 0 ? "SavedVariables cleared." : "No SavedVariables files were found.");
  }, "savedvariables-clear");
}

function requestManualBackup() {
  if (!state.settings?.backup_dir_override) {
    state.manualBackupError = "Choose a backup folder to enable manual backups.";
    render();
    return;
  }
  state.manualBackupError = null;
  state.restoreResult = null;
  state.manualBackupConfirmOpen = true;
  state.manualBackupIncludeSavedVariables = false;
  render();
}

function cancelManualBackup() {
  if (guardedOperationRunning()) return;
  state.manualBackupConfirmOpen = false;
  render();
}

function confirmManualBackup() {
  const backupDir = state.settings?.backup_dir_override;
  if (!backupDir) {
    state.manualBackupError = "Choose a backup folder to enable manual backups.";
    state.manualBackupConfirmOpen = false;
    render();
    return;
  }

  state.manualBackupResult = null;
  state.manualBackupError = null;
  return withLoading(async () => {
    const result = await invoke<BackupResult>("create_compressed_backup", {
      addonsPath: effectiveAddonsPath(),
      backupDir,
      includeSavedVariables: state.manualBackupIncludeSavedVariables,
    });
    state.manualBackupResult = result;
    state.manualBackupError = null;
    state.restoreResult = null;
    state.manualBackupConfirmOpen = false;
  }, "manual-backup");
}

async function requestRestoreBackup() {
  if (!state.settings?.backup_dir_override) {
    state.manualBackupError = "Choose a backup folder to enable restore.";
    render();
    return;
  }

  state.manualBackupError = null;
  state.manualBackupResult = null;
  state.restoreResult = null;
  const selected = await browseForBackupZip(state.settings.backup_dir_override);
  if (!selected) return;

  if (!selected.toLowerCase().endsWith(".zip")) {
    state.manualBackupError = "Only ZIP backups are supported.";
    render();
    return;
  }

  return withLoading(async () => {
    const inspection = await invoke<BackupInspection>("inspect_backup_zip", {
      zipPath: selected,
      addonsPath: effectiveAddonsPath(),
    });
    if (!inspection.valid) {
      state.manualBackupError = inspection.warnings[0] || "Backup ZIP is invalid.";
      return;
    }

    state.restoreZipPath = selected;
    state.restoreInspection = inspection;
    state.restoreAddons = inspection.contains_addons;
    state.restoreSavedVariables = false;
  }, "backup-restore");
}

function cancelRestoreBackup() {
  if (guardedOperationRunning()) return;
  state.restoreZipPath = null;
  state.restoreInspection = null;
  render();
}

function confirmRestoreBackup() {
  const zipPath = state.restoreZipPath;
  const inspection = state.restoreInspection;
  if (!zipPath || !inspection) return;
  if (!state.restoreAddons && !state.restoreSavedVariables) {
    state.manualBackupError = "Choose at least one folder to restore.";
    render();
    return;
  }

  return withLoading(async () => {
    const result = await invoke<RestoreResult>("restore_backup_zip", {
      zipPath,
      addonsPath: effectiveAddonsPath(),
      restoreAddons: state.restoreAddons,
      restoreSavedVariables: state.restoreSavedVariables,
    });
    state.restoreResult = result;
    state.restoreZipPath = null;
    state.restoreInspection = null;
    state.manualBackupResult = null;
    state.manualBackupError = null;
    await refreshInstalledData(true);
  }, "backup-restore");
}

async function openCreatedBackupFolder() {
  const result = state.manualBackupResult;
  if (!result) return;
  state.error = null;
  state.manualBackupError = null;
  render();
  try {
    await invoke("open_path_location", { path: result.backup_zip_path });
  } catch (error) {
    state.manualBackupError = `Could not open backup location. ${error instanceof Error ? error.message : String(error)}`;
    render();
  }
}

async function openAddonFolder(local: LocalAddon) {
  state.error = null;
  clearSuccess();
  render();
  try {
    await invoke("open_addon_folder", {
      folderName: local.folder_name,
      path: effectiveAddonsPath(),
    });
  } catch (error) {
    state.error = `Could not open addon folder. ${error instanceof Error ? error.message : String(error)}`;
    render();
  }
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

function syncSelectedStateAfterUpdate(target?: string) {
  const folderName = target ?? state.selectedLocal?.folder_name ?? null;
  if (!folderName || state.selectedLocal?.folder_name.toLowerCase() !== folderName.toLowerCase()) return;
  state.selectedLocal = installedLocalByFolder(folderName) ?? state.selectedLocal;
  state.selectedMatch = state.updates?.matches.find((match) => match.local.folder_name.toLowerCase() === folderName.toLowerCase()) ?? null;
  if (state.selectedLocal) {
    state.selectedDependenciesLoading = true;
    void loadInstalledDependencies(state.selectedLocal.folder_name);
  }
}

function installedLocalFromResult(result: InstallRemoteAddonResponse) {
  const mainFolders = new Set(
    result.plan.items
      .filter((item) => item.action === "would-install-new" || item.action === "would-replace-existing")
      .map((item) => folderNameFromPath(item.target_folder))
      .filter(Boolean)
      .map((folder) => folder!.toLowerCase()),
  );
  const folderName = result.items
    .filter((item) => item.action === "installed-new" || item.action === "replaced-existing")
    .map((item) => folderNameFromPath(item.target_folder))
    .find((folder) => folder && mainFolders.has(folder.toLowerCase()));

  if (!folderName) return null;
  return state.installed?.addons.find((addon) => addon.folder_name.toLowerCase() === folderName.toLowerCase()) ?? null;
}

function folderNameFromPath(value: string | null) {
  if (!value) return null;
  const parts = value.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? null;
}

async function applyUpdateAll() {
  state.updateAllPlan = null;
  state.updateAllResult = null;
  state.singleUpdatePlan = null;
  state.singleUpdateResult = null;
  state.removeResult = null;
  state.updateAllProgress = null;
  state.loading = true;
  state.operation = "update-all-apply";
  state.operationTarget = null;
  state.error = null;
  clearSuccess();
  closeAddonContextMenu(false);
  render();

  let unlistenProgress: (() => void) | null = null;
  try {
    unlistenProgress = await listen<UpdateAllProgress>(UPDATE_ALL_PROGRESS_EVENT, (event) => {
      state.updateAllProgress = event.payload;
      render();
    });

    state.updateAllResult = await invoke<ApplyUpdateAllResponse>("apply_update_all", {
      path: effectiveAddonsPath(),
      backupDir: state.settings?.backup_dir_override || null,
      keepDownload: state.settings?.keep_downloads_default ?? false,
      downloadDir: state.settings?.download_dir || null,
      includeUnknown: updateIncludeUnknownDefault(),
      limit: null,
    });
    state.path = state.updateAllResult.addons_dir;
    state.updateAllPlan = null;
    setSuccess(state.updateAllResult.failure ? null : updateAllSuccessMessage(state.updateAllResult));
    await refreshInstalledData(true);
    syncSelectedStateAfterUpdate();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    state.error = operationErrorMessage("update-all-apply", null, message);
  } finally {
    unlistenProgress?.();
    state.loading = false;
    state.operation = null;
    state.operationTarget = null;
    state.updateAllProgress = null;
    render();
  }
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

function installedItems(): InstalledViewModel[] {
  const matches = state.updates?.matches ?? [];
  return (state.installed?.addons ?? []).map((addon) => ({
    addon,
    match: matches.find((match) => match.local.folder_name === addon.folder_name) ?? null,
  }));
}

function installedView(): InstalledViewModel[] {
  const query = state.installedQuery.trim().toLowerCase();

  return installedItems()
    .filter((item) => {
      const status = installedStatus(item.match, item.addon);
      if (!shouldShowInstalledAddon(item, state.settings?.hide_libraries_in_installed ?? false, isActionableInstalledUpdate)) return false;
      if (state.installedFilter === "update" && status.kind !== "reliable-update") return false;
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
  return plainEsoText(item.match?.remote?.name ?? item.addon.title ?? item.addon.folder_name);
}

function dateValue(value: string | null | undefined) {
  return value ? Date.parse(value) || 0 : 0;
}

function installedStatus(match: MatchResult | null, addon: LocalAddon) {
  if (!addon.valid_manifest) return { label: "Invalid folder", kind: "invalid", rank: 3 };
  if (match?.update_confidence === "current") return { label: "Current", kind: "current", rank: 4 };
  if (!match) return { label: "Unknown", kind: "unknown", rank: 2 };
  if (isActionableUpdate(match)) return { label: "Update candidate", kind: "reliable-update", rank: 1 };
  if (match.update_confidence === "possible-update") return { label: "Version check uncertain", kind: "possible-update", rank: 2 };
  if (match.update_confidence === "local-newer") return { label: "Local newer", kind: "local-newer", rank: 5 };
  if (addon.is_library === true) return { label: "Unknown", kind: "unknown", rank: 2 };
  if (match.status === "possible-update") return { label: "Version differs", kind: "possible-update", rank: 2 };
  if (match.status === "unknown-update") return { label: "Unknown", kind: "unknown", rank: 2 };
  if (match.status === "no-match") return { label: "Not found", kind: "not-found", rank: 3 };
  if (match.status === "ambiguous") return { label: "Ambiguous", kind: "ambiguous", rank: 3 };
  if (match.status === "matched") return { label: "Current", kind: "current", rank: 4 };
  if (match.status === "local-newer") return { label: "Local newer", kind: "local-newer", rank: 5 };
  return { label: "Unknown", kind: "unknown", rank: 2 };
}

function renderInstalledCardActions(item: InstalledViewModel, status: { label: string; kind: string }) {
  const resolveAction = ["not-found", "ambiguous"].includes(status.kind)
    ? `<button class="warning-action small" data-resolve-folder="${escapeAttr(item.addon.folder_name)}" ${disabledAttr()}>${loadingButtonContent("Resolve", "Searching...", "resolve-search", item.addon.folder_name)}</button>`
    : "";
  return `${resolveAction}${renderCardUpdateAction(item.match)}`;
}

function renderCardUpdateAction(match: MatchResult | null) {
  if (!match) return "";
  if (isActionableUpdate(match)) {
    return `<button class="primary small" data-apply-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>${loadingButtonContent("Update", singleUpdateButtonLoadingLabel(match.local.folder_name), "update-apply", match.local.folder_name)}</button>`;
  }
  if (state.forceUpdate && ["matched", "unknown-update", "local-newer"].includes(match.status)) {
    return `<button class="secondary small" data-apply-update-target="${escapeAttr(match.local.folder_name)}" ${disabledAttr()}>${loadingButtonContent("Reinstall", singleUpdateButtonLoadingLabel(match.local.folder_name), "update-apply", match.local.folder_name)}</button>`;
  }
  return "";
}

function installedStatusNote(status: { label: string; kind: string }, match?: MatchResult | null) {
  if (match?.update_reason === "Remote metadata unavailable") return "Remote metadata unavailable";
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

function singleUpdateButtonLoadingLabel(target: string) {
  if (!isOperation("update-apply", target)) return "Updating...";
  return state.singleUpdatePhase === "preparing" ? "Preparing update..." : "Updating...";
}

function updateAllButtonLoadingLabel() {
  if (!isOperation("update-all-apply")) return "Updating...";
  return state.updateAllProgress ? "Updating..." : "Preparing update...";
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

function formatBytesDisplay(value: number | null | undefined) {
  if (value === null || value === undefined) return "-";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return unitIndex === 0 ? `${value} ${units[unitIndex]}` : `${size.toFixed(1)} ${units[unitIndex]}`;
}

function formatBackupDate(value: string | null) {
  if (!value) return "Unknown";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}

function yesNo(value: boolean) {
  return value ? "Yes" : "No";
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
      [
        "install-plan",
        "install-apply",
        "dependency-install",
        "resolve-search",
        "resolve-link",
        "resolve-reinstall",
        "update-apply",
        "remove-apply",
        "savedvariables-clear",
        "manual-backup",
        "backup-restore",
        "update-all-apply",
      ].includes(
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

async function initializeApp() {
  state.startupViewReady = false;
  state.startupFatalError = null;
  state.loading = true;
  state.operation = "startup";
  state.operationTarget = null;
  state.error = null;
  clearSuccess();
  closeAddonContextMenu(false);
  render();

  try {
    await loadStartup();
    if (!state.needsInitialSetup) {
      state.operation = "installed";
      render();
      await refreshInstalledData(false);
    }
    state.startupViewReady = true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    state.startupFatalError =
      state.operation === "installed" ? `Could not prepare your addons. ${message}` : `Could not load startup settings. ${message}`;
  } finally {
    state.loading = false;
    state.operation = null;
    state.operationTarget = null;
    render();
  }
}

async function loadStartup() {
  const startup = await invoke<AppStartupInfo>("get_startup_info");
  state.settings = startup.settings;
  state.detectedAddonsPath = startup.detected_addons_dir;
  state.needsInitialSetup = !startup.settings_exists;
  state.setupAddonsPath = startup.settings.addons_dir_override ?? startup.detected_addons_dir ?? "";
  applySettingsToState(startup.settings);
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
  const previousAddonsDir = state.settings?.addons_dir_override ?? null;
  const previousBackupDir = state.settings?.backup_dir_override ?? null;
  state.settings = readSettingsDraft();
  if (
    (state.settings.addons_dir_override ?? null) !== previousAddonsDir ||
    (state.settings.backup_dir_override ?? null) !== previousBackupDir
  ) {
    state.manualBackupResult = null;
    state.manualBackupError = null;
    state.restoreResult = null;
    state.restoreInspection = null;
    state.restoreZipPath = null;
  }
  state.addonsPathExists = null;
}

function readSettingsDraft(): AppSettings {
  const current = state.settings;
  return {
    addons_dir_override: valueOrCurrent(`#${TEXT_INPUT_IDS.settingsAddonsPath}`, current?.addons_dir_override ?? null),
    backup_dir_override: valueOrCurrent(`#${TEXT_INPUT_IDS.settingsBackupFolder}`, current?.backup_dir_override ?? null),
    download_dir: valueOrCurrent(`#${TEXT_INPUT_IDS.settingsDownloadFolder}`, current?.download_dir ?? null),
    keep_downloads_default: checkedOrCurrent("#settings-keep-downloads", current?.keep_downloads_default ?? false),
    include_unknown_updates_default: checkedOrCurrent("#settings-include-unknown", current?.include_unknown_updates_default ?? false),
    hide_libraries_in_search: checkedOrCurrent("#settings-hide-libraries-search", current?.hide_libraries_in_search ?? false),
    hide_libraries_in_installed: checkedOrCurrent("#settings-hide-libraries-installed", current?.hide_libraries_in_installed ?? false),
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
      hide_libraries_in_search: state.settings?.hide_libraries_in_search ?? false,
      hide_libraries_in_installed: state.settings?.hide_libraries_in_installed ?? false,
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
    invalidateSearchResults();
    state.settings = saved;
    applySettingsToState(saved);
    state.addonsPathExists = await invoke<boolean>("path_exists", { path: effectiveAddonsPath() });
  }, "settings");
}

function resetSettings() {
  return withLoading(async () => {
    const reset = await invoke<AppSettings>("reset_app_settings");
    invalidateSearchResults();
    state.settings = reset;
    applySettingsToState(reset);
    state.addonsPathExists = await invoke<boolean>("path_exists", { path: effectiveAddonsPath() });
  }, "settings");
}

function invalidateSearchResults() {
  state.searchLoaded = false;
  state.searchLoadAttempted = false;
  state.searchResults = [];
  state.totalSearchMatches = 0;
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
    setSuccess("Cache cleared.");
  }, "cache");
}

function settingField(
  label: string,
  id: string,
  value: string,
  options: { browse?: boolean; helper?: string; placeholder?: string } = {},
) {
  const browse = options.browse ?? false;
  return `
    <div class="field setting-item">
      <label for="${escapeAttr(id)}">${escapeHtml(label)}</label>
      ${options.helper ? `<p class="setting-helper">${escapeHtml(options.helper)}</p>` : ""}
      <div class="${browse ? "field-with-action" : ""}">
        ${textInput(id, value, { placeholder: options.placeholder ?? "Leave blank for default" })}
        ${browse ? `<button class="secondary icon-button browse-button" data-browse-target="${escapeAttr(id)}" title="Browse for ${escapeAttr(label)}" ${disabledAttr()}>${icon("folder")} Browse</button>` : ""}
      </div>
    </div>
  `;
}

function settingToggle(label: string, id: string, value: boolean, helper: string) {
  return `
    <div class="setting-toggle-row">
      <label class="setting-toggle" for="${escapeAttr(id)}">
        <input class="toggle-input" type="checkbox" ${checkboxInputAttrs(id)} ${value ? "checked" : ""} ${disabledAttr()} />
        <span class="setting-toggle-copy">
          <span class="setting-toggle-label">${escapeHtml(label)}</span>
          <span class="setting-helper">${escapeHtml(helper)}</span>
        </span>
        <span class="toggle-switch" aria-hidden="true"></span>
      </label>
    </div>
  `;
}

function textInput(id: string, value: string, options: { name?: string; placeholder?: string } = {}) {
  const name = options.name ?? id;
  return `<input type="text" id="${escapeAttr(id)}" name="${escapeAttr(name)}" value="${escapeAttr(value)}" placeholder="${escapeAttr(options.placeholder ?? "")}" ${noAutocompleteAttrs} ${disabledAttr()} />`;
}

function checkboxInputAttrs(id: string, name = id) {
  return `id="${escapeAttr(id)}" name="${escapeAttr(name)}" ${noAutocompleteAttrs}`;
}

function valueOrCurrent(selector: string, current: string | null) {
  const input = document.querySelector<HTMLInputElement>(selector);
  if (!input) return current;
  return input.value.trim() || null;
}

function checkedOrCurrent(selector: string, current: boolean) {
  const input = document.querySelector<HTMLInputElement>(selector);
  if (!input) return current;
  return input.checked;
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
    installed: '<path d="m21 8-9-5-9 5 9 5 9-5Z"/><path d="m3 8 9 5 9-5"/><path d="M3 8v8l9 5 9-5V8"/><path d="M12 13v8"/>',
    rotate: '<path d="M3 12a9 9 0 1 0 3-6.7"/><path d="M3 3v6h6"/>',
    search: '<circle cx="11" cy="11" r="7"/><path d="m20 20-3.5-3.5"/>',
    settings: '<path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06A1.7 1.7 0 0 0 15 19.37a1.7 1.7 0 0 0-1 .58 1.7 1.7 0 0 0-.4 1.08V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-.4-1.08 1.7 1.7 0 0 0-1-.58 1.7 1.7 0 0 0-1.88.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 3.63 15a1.7 1.7 0 0 0-.58-1 1.7 1.7 0 0 0-1.08-.4H2a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.08-.4 1.7 1.7 0 0 0 .58-1 1.7 1.7 0 0 0-.34-1.88l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 8 4.63a1.7 1.7 0 0 0 1-.58 1.7 1.7 0 0 0 .4-1.08V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 .4 1.08 1.7 1.7 0 0 0 1 .58 1.7 1.7 0 0 0 1.88-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 20.37 9a1.7 1.7 0 0 0 .58 1 1.7 1.7 0 0 0 1.08.4H22a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.08.4 1.7 1.7 0 0 0-.58 1Z"/>',
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

void initializeApp();
