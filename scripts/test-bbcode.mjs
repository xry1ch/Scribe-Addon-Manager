import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { transformWithEsbuild } from "vite";

const source = await readFile(new URL("../src-ui/bbcode.ts", import.meta.url), "utf8");
const transformed = await transformWithEsbuild(source, "bbcode.ts", {
  format: "esm",
  loader: "ts",
  target: "es2020",
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString("base64")}`;
const { renderEsoMarkup, stripEsoMarkup } = await import(moduleUrl);

function assertIncludes(name, actual, expected) {
  assert.ok(actual.includes(expected), `${name}: expected output to include ${expected}\n\n${actual}`);
}

function assertExcludes(name, actual, unexpected) {
  assert.ok(!actual.includes(unexpected), `${name}: expected output not to include ${unexpected}\n\n${actual}`);
}

const fontOutput = renderEsoMarkup('[FONT="System"]AI assistance was involved[/FONT]');
assertIncludes("FONT wrapper preserves content", fontOutput, "AI assistance was involved");
assertExcludes("FONT wrapper strips raw tag", fontOutput, "[FONT");

const headingOutput = renderEsoMarkup('[SIZE="4"][I][B][COLOR="Orange"]What it includes:[/COLOR][/B][/I][/SIZE]');
assertIncludes("nested SIZE heading uses heading element", headingOutput, '<h3 class="eso-heading">');
assertIncludes("nested SIZE heading preserves text", headingOutput, "What it includes:");

const subheadingOutput = renderEsoMarkup('[SIZE="3"][COLOR="Wheat"][U][I]Resource Bars[/I][/U][/COLOR][/SIZE]');
assertIncludes("SIZE 3 uses subheading element", subheadingOutput, '<h4 class="eso-subheading">');
assertIncludes("SIZE 3 preserves text", subheadingOutput, "Resource Bars");

const listOutput = renderEsoMarkup("[LIST]\n[*]Adds custom health...\n[/LIST]");
assertIncludes("LIST renders ul", listOutput, '<ul class="eso-list">');
assertIncludes("LIST renders item", listOutput, "<li>Adds custom health...</li>");
assertExcludes("LIST strips raw item marker", listOutput, "[*]");

const safeImageOutput = renderEsoMarkup("[IMG]https://example.com/a.png[/IMG]");
assertIncludes("safe IMG renders lightbox button", safeImageOutput, 'class="bbcode-image-frame"');
assertIncludes("safe IMG preserves URL", safeImageOutput, 'data-lightbox-url="https://example.com/a.png"');

const unsafeImageOutput = renderEsoMarkup("[IMG]javascript:alert(1)[/IMG]");
assertExcludes("unsafe IMG rejects scheme", unsafeImageOutput, "javascript:");
assertExcludes("unsafe IMG strips raw tag", unsafeImageOutput, "[IMG]");

const urlOutput = renderEsoMarkup('[URL="https://example.com"]label[/URL]');
assertIncludes("safe URL renders link", urlOutput, 'href="https://example.com/"');
assertIncludes("safe URL preserves label", urlOutput, ">label</a>");

const unknownOutput = renderEsoMarkup("[UNKNOWN]inner text[/UNKNOWN]");
assertIncludes("unknown tag preserves text", unknownOutput, "inner text");
assertExcludes("unknown tag strips raw tag", unknownOutput, "[UNKNOWN]");

const pipeColorOutput = renderEsoMarkup("|cFF0088Colored text|r");
assertIncludes("ESO pipe color preserves text", pipeColorOutput, "Colored text");
assertExcludes("ESO pipe color strips raw marker", pipeColorOutput, "|cFF0088");

const nirnSteelSample = `[FONT="System"][I][COLOR="Wheat"]AI assistance was involved in reviewing and updating parts of this addon code.[/COLOR][/I][/FONT]

[IMG]https://i.imgur.com/XQEogi8.png[/IMG]

[I][B][FONT="Tahoma"][COLOR="Teal"][SIZE="4"]NirnSteel UI[/SIZE][/COLOR][/FONT][/B][/I] [FONT="Verdana"]is my personal take on making the ESO HUD feel a bit cleaner.[/FONT]

[SIZE="4"][I][B][COLOR="Orange"]What it includes:[/COLOR][/B][/I][/SIZE]

[SIZE="3"][COLOR="Wheat"][U][I]Resource Bars[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Adds custom health...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Action Bar Frames[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Changes the action bar frame style...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Loot History[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Custom loot popups with sound feedback...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Experience Tracker[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Replaces the stock XP / Champion bar...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Cast Bar[/I][/U][/COLOR][/SIZE]

[LIST]
[*]A custom cast bar with a cleaner look...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Damage Numbers[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Custom combat text with font and effect options...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Kill Sound[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Plays a short sound when you land a killing blow...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Adventure Camera[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Camera settings for exploration and combat...
[/LIST]

[SIZE="3"][COLOR="Wheat"][U][I]Compass[/I][/U][/COLOR][/SIZE]

[LIST]
[*]Compass styling with cleaner markers...
[/LIST]

All modules can be turned on or off individually.

Any suggestions for new modules or ideas are welcome.

[SIZE="3"]Dependencies[/SIZE]

[LIST]
[*]LibAddonMenu-2.0 r41 or newer
[/LIST]`;

const nirnSteelOutput = renderEsoMarkup(nirnSteelSample);
for (const expected of [
  "AI assistance was involved",
  "NirnSteel UI",
  "is my personal take",
  "What it includes:",
  "Resource Bars",
  "Adds custom health...",
  "Action Bar Frames",
  "Changes the action bar frame style...",
  "Loot History",
  "Experience Tracker",
  "Cast Bar",
  "Damage Numbers",
  "Kill Sound",
  "Adventure Camera",
  "Compass",
  "All modules can be turned on or off",
  "Any suggestions for new modules or ideas are welcome.",
  "Dependencies",
  "LibAddonMenu-2.0 r41 or newer",
]) {
  assertIncludes("NirnSteel sample", nirnSteelOutput, expected);
}
for (const rawTag of ["[FONT", "[/FONT", "[SIZE", "[/SIZE", "[COLOR", "[/COLOR", "[B", "[/B", "[I", "[/I", "[U", "[/U", "[LIST", "[/LIST", "[*]", "[IMG]", "[/IMG]"]) {
  assertExcludes("NirnSteel sample strips raw tags", nirnSteelOutput, rawTag);
}

const stripped = stripEsoMarkup('[FONT="Tahoma"][UNKNOWN]NirnSteel UI[/UNKNOWN][/FONT]');
assert.equal(stripped, "NirnSteel UI");

console.log("BBCode renderer tests passed.");
