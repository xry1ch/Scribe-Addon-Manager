import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { transformWithEsbuild } from "vite";

const source = await readFile(new URL("../src-ui/libraryFilters.ts", import.meta.url), "utf8");
const transformed = await transformWithEsbuild(source, "libraryFilters.ts", {
  format: "esm",
  loader: "ts",
  target: "es2020",
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString("base64")}`;
const {
  shouldShowSearchAddon,
  shouldShowInstalledAddon,
} = await import(moduleUrl);

function remoteAddon(overrides = {}) {
  return {
    uid: "1",
    name: "Inventory Helper",
    author_name: "Author",
    version: "1.0.0",
    updated_display: "2026-01-01",
    file_info_url: null,
    summary: null,
    directories: ["InventoryHelper"],
    category_id: "20",
    category_name: "Bags, Bank, Inventory",
    downloads: 10,
    monthly_downloads: 1,
    is_library: false,
    image_urls: [],
    thumbnail_urls: [],
    installed: false,
    installed_local: null,
    installed_match: null,
    ...overrides,
  };
}

function localAddon(overrides = {}) {
  return {
    folder_name: "InventoryHelper",
    folder_path: "/AddOns/InventoryHelper",
    title: "Inventory Helper",
    author: "Author",
    display_version: "1.0.0",
    api_versions: [],
    depends_on: [],
    optional_depends_on: [],
    saved_variables: [],
    saved_variables_per_character: [],
    description: null,
    is_library: false,
    valid_manifest: true,
    ...overrides,
  };
}

function match(local, overrides = {}) {
  return {
    local,
    status: "matched",
    update_confidence: "current",
    update_reason: "versions match",
    managed: true,
    remote: remoteCandidate(),
    ...overrides,
  };
}

function remoteCandidate(overrides = {}) {
  return {
    uid: "1",
    name: "Inventory Helper",
    author_name: "Author",
    version: "1.0.0",
    updated_display: "2026-01-01",
    file_info_url: null,
    summary: null,
    directories: ["InventoryHelper"],
    category_id: "20",
    category_name: "Bags, Bank, Inventory",
    downloads: 10,
    monthly_downloads: 1,
    is_library: false,
    image_urls: [],
    thumbnail_urls: [],
    ...overrides,
  };
}

const categories = [
  { id: "20", name: "Bags, Bank, Inventory", parent_id: null },
  { id: "53", name: "Libraries", parent_id: null },
];

const librarySearchResult = remoteAddon({
  uid: "53",
  name: "LibAddonMenu-2.0",
  directories: ["LibAddonMenu-2.0"],
  category_id: "53",
  category_name: "Libraries",
  is_library: true,
});

assert.equal(
  shouldShowSearchAddon(librarySearchResult, {
    hideLibraries: true,
    selectedCategoryId: "",
    categories,
    query: "",
  }),
  false,
  "search filtering hides library results",
);

assert.equal(
  shouldShowSearchAddon(librarySearchResult, {
    hideLibraries: true,
    selectedCategoryId: "53",
    categories,
    query: "",
  }),
  true,
  "search filtering keeps the selected Libraries category visible",
);

assert.equal(
  shouldShowSearchAddon(librarySearchResult, {
    hideLibraries: true,
    selectedCategoryId: "",
    categories,
    query: "LibAddonMenu-2.0",
  }),
  true,
  "search filtering keeps exact library queries visible",
);

assert.equal(
  shouldShowSearchAddon(remoteAddon(), {
    hideLibraries: true,
    selectedCategoryId: "",
    categories,
    query: "",
  }),
  true,
  "search filtering keeps non-library addons visible",
);

const libraryLocal = localAddon({
  folder_name: "LibAddonMenu-2.0",
  title: "LibAddonMenu-2.0",
  is_library: true,
});
const currentLibrary = { addon: libraryLocal, match: match(libraryLocal) };
const updatingLibrary = {
  addon: libraryLocal,
  match: match(libraryLocal, { status: "possible-update", update_confidence: "reliable-update" }),
};

function isActionableUpdate(item) {
  return item.match?.update_confidence === "reliable-update";
}

assert.equal(
  shouldShowInstalledAddon(currentLibrary, true, isActionableUpdate),
  false,
  "installed filtering hides current libraries",
);

assert.equal(
  shouldShowInstalledAddon(updatingLibrary, true, isActionableUpdate),
  true,
  "installed filtering keeps libraries with actionable updates",
);

assert.equal(
  shouldShowInstalledAddon({ addon: localAddon(), match: match(localAddon()) }, true, isActionableUpdate),
  true,
  "installed filtering keeps non-library addons visible",
);

console.log("Library filter tests passed.");
