import type { AddonSummary, LocalAddon, MatchResult, PlannedAction, RemoteCategory, RemoteCandidate } from "./types";

const LIBRARIES_CATEGORY_ID = "53";

type RemoteLike = Pick<RemoteCandidate, "name" | "directories" | "category_id" | "category_name" | "is_library">;

export interface InstalledLibraryFilterItem {
  addon: LocalAddon;
  match: MatchResult | null;
}

export function shouldShowSearchAddon(
  addon: AddonSummary,
  options: {
    hideLibraries: boolean;
    selectedCategoryId: string;
    categories: RemoteCategory[];
    query: string;
  },
) {
  if (!options.hideLibraries) return true;
  if (isLibrariesCategorySelected(options.selectedCategoryId, options.categories)) return true;
  if (!isSearchResultLibrary(addon)) return true;
  return exactAddonQueryMatches(addon, options.query);
}

export function shouldShowInstalledAddon(
  item: InstalledLibraryFilterItem,
  hideLibraries: boolean,
  actions: PlannedAction[],
) {
  if (!hideLibraries) return true;
  if (!isInstalledAddonLibrary(item)) return true;
  return hasActionableUpdate(item.addon.folder_name, actions);
}

export function isSearchResultLibrary(addon: AddonSummary) {
  return (
    addon.is_library ||
    addon.installed_local?.is_library === true ||
    addon.installed_match?.local.is_library === true ||
    isRemoteLibrary(addon)
  );
}

export function isInstalledAddonLibrary(item: InstalledLibraryFilterItem) {
  return item.addon.is_library === true || Boolean(item.match?.remote && isRemoteLibrary(item.match.remote));
}

export function isRemoteLibrary(remote: RemoteLike) {
  if (remote.is_library) return true;
  if (isLibraryCategory(remote.category_id, remote.category_name)) return true;
  if (hasCategorySignal(remote)) return false;
  return weakLibraryNameSignal(remote.name, remote.directories);
}

export function isLibraryCategory(categoryId: string | null, categoryName: string | null) {
  return categoryId?.trim() === LIBRARIES_CATEGORY_ID || normalizeCategory(categoryName).includes("librar");
}

function isLibrariesCategorySelected(categoryId: string, categories: RemoteCategory[]) {
  const selected = categoryId.trim();
  if (!selected) return false;
  if (isLibraryCategory(selected, null)) return true;
  const category = categories.find((item) => item.id === selected);
  return category ? isLibraryCategory(category.id, category.name) : false;
}

function hasActionableUpdate(folderName: string, actions: PlannedAction[]) {
  const normalizedFolder = folderName.toLowerCase();
  return actions.some(
    (action) =>
      action.local_folder.toLowerCase() === normalizedFolder &&
      action.action === "would-update" &&
      action.update_confidence === "reliable-update",
  );
}

function exactAddonQueryMatches(addon: AddonSummary, query: string) {
  const normalizedQuery = normalizeAddonLookup(query);
  if (!normalizedQuery) return false;
  return (
    normalizeAddonLookup(addon.name ?? "") === normalizedQuery ||
    addon.directories.some((directory) => normalizeAddonLookup(directory) === normalizedQuery)
  );
}

function hasCategorySignal(remote: RemoteLike) {
  return Boolean(remote.category_id?.trim() || remote.category_name?.trim());
}

function weakLibraryNameSignal(name: string | null, directories: string[]) {
  return startsWithLib(name) || directories.some(startsWithLib);
}

function startsWithLib(value: string | null) {
  return value?.trimStart().toLowerCase().startsWith("lib") ?? false;
}

function normalizeCategory(value: string | null) {
  return (value ?? "")
    .toLowerCase()
    .replace(/&/g, "and")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function normalizeAddonLookup(value: string) {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .trim()
    .replace(/\s+/g, " ");
}
