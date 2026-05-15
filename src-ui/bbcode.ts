type BbNode = TextNode | TagNode;

interface TextNode {
  type: "text";
  text: string;
}

interface TagNode {
  type: "tag";
  name: string;
  attr: string | null;
  children: BbNode[];
}

const blockTags = new Set(["LIST", "IMG", "HR", "QUOTE", "CODE", "CENTER", "LEFT", "RIGHT"]);
const transparentInlineTags = new Set(["FONT"]);
const styleWrapperTags = new Set(["B", "I", "U", "S", "COLOR", "FONT"]);
const safeNamedColors: Record<string, string> = {
  aqua: "aqua",
  black: "black",
  blue: "blue",
  cyan: "cyan",
  fuchsia: "fuchsia",
  gold: "gold",
  gray: "gray",
  green: "green",
  grey: "grey",
  lime: "lime",
  magenta: "magenta",
  orange: "orange",
  purple: "purple",
  red: "red",
  silver: "silver",
  teal: "teal",
  wheat: "wheat",
  white: "white",
  yellow: "yellow",
};

export function renderEsoMarkup(value: string) {
  const output = renderNodesBlock(parseBbCode(value));
  return output || "";
}

export function renderInlineEsoMarkup(value: string) {
  return renderInlineNodes(parseBbCode(value));
}

export function stripEsoMarkup(value: string) {
  return stripEsoPipeColorText(textContent(parseBbCode(value))).replace(/[ \t]+\n/g, "\n").trim();
}

function parseBbCode(value: string): BbNode[] {
  const root: TagNode = { type: "tag", name: "ROOT", attr: null, children: [] };
  const stack: TagNode[] = [root];
  const tagPattern = /\[(\/?)([A-Za-z*]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\]\s]+)))?(?:\s+[^\]]*)?\]/g;
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = tagPattern.exec(value)) !== null) {
    appendText(stack, value.slice(cursor, match.index));
    cursor = match.index + match[0].length;

    const closing = match[1] === "/";
    const name = match[2].toUpperCase();
    const attr = match[3] ?? match[4] ?? match[5] ?? null;

    if (name === "*") {
      openListItem(stack);
      continue;
    }

    if (closing) {
      closeTag(stack, name);
      continue;
    }

    const node: TagNode = { type: "tag", name, attr, children: [] };
    currentNode(stack).children.push(node);
    if (name !== "HR") {
      stack.push(node);
    }
  }

  appendText(stack, value.slice(cursor));
  return root.children;
}

function appendText(stack: TagNode[], text: string) {
  if (text.length === 0) return;
  currentNode(stack).children.push({ type: "text", text });
}

function currentNode(stack: TagNode[]) {
  return stack[stack.length - 1];
}

function openListItem(stack: TagNode[]) {
  let listIndex = -1;
  for (let index = stack.length - 1; index >= 0; index -= 1) {
    if (stack[index].name === "LIST") {
      listIndex = index;
      break;
    }
  }

  if (listIndex === -1) {
    const list: TagNode = { type: "tag", name: "LIST", attr: null, children: [] };
    currentNode(stack).children.push(list);
    stack.push(list);
    listIndex = stack.length - 1;
  } else {
    stack.length = listIndex + 1;
  }

  const item: TagNode = { type: "tag", name: "LI", attr: null, children: [] };
  stack[listIndex].children.push(item);
  stack.push(item);
}

function closeTag(stack: TagNode[], name: string) {
  for (let index = stack.length - 1; index > 0; index -= 1) {
    if (stack[index].name === name) {
      stack.length = index;
      return;
    }
  }
}

function renderNodesBlock(nodes: BbNode[]): string {
  let html = "";
  let paragraph: BbNode[] = [];

  const flushParagraph = () => {
    const trimmed = trimParagraphNodes(paragraph);
    paragraph = [];
    if (!hasVisibleContent(trimmed)) return;

    const heading = standaloneHeadingLevel(trimmed);
    if (heading === 4) {
      html += `<h3 class="eso-heading">${renderInlineNodes(trimmed)}</h3>`;
      return;
    }
    if (heading === 3) {
      html += `<h4 class="eso-subheading">${renderInlineNodes(trimmed)}</h4>`;
      return;
    }

    html += `<p>${renderInlineNodes(trimmed).replace(/\n/g, "<br />")}</p>`;
  };

  for (const node of nodes) {
    if (node.type === "text") {
      const chunks = node.text.replace(/\r\n?/g, "\n").replace(/\[BR\]/gi, "\n").split(/(\n[ \t]*\n+)/);
      for (const chunk of chunks) {
        if (/^\n[ \t]*\n+$/.test(chunk)) {
          flushParagraph();
        } else if (chunk.length > 0) {
          paragraph.push({ type: "text", text: chunk });
        }
      }
      continue;
    }

    if (isBlockNode(node) || containsBlockNode(node)) {
      flushParagraph();
      html += renderBlockNode(node);
    } else {
      paragraph.push(node);
    }
  }

  flushParagraph();
  return html;
}

function renderBlockNode(node: TagNode): string {
  if (node.name === "LIST") {
    const items = listItems(node);
    return items.length > 0 ? `<ul class="eso-list">${items.map((item) => `<li>${renderInlineNodes(trimParagraphNodes(item.children))}</li>`).join("")}</ul>` : "";
  }

  if (node.name === "IMG") {
    return renderImageNode(node);
  }

  if (node.name === "HR") {
    return '<hr class="eso-rule" />';
  }

  if (node.name === "QUOTE") {
    return `<blockquote class="eso-quote">${renderNodesBlock(node.children)}</blockquote>`;
  }

  if (node.name === "CODE") {
    return `<pre class="eso-code"><code>${escapeHtml(textContent(node.children).trim())}</code></pre>`;
  }

  if (["CENTER", "LEFT", "RIGHT"].includes(node.name)) {
    return `<div class="eso-align ${node.name.toLowerCase()}">${renderNodesBlock(node.children)}</div>`;
  }

  if (node.name === "URL") {
    const body = renderNodesBlock(node.children);
    const href = safeUrl(node.attr);
    const link = href ? `<p><a href="${escapeAttr(href)}" target="_blank" rel="noreferrer noopener">Open link</a></p>` : "";
    return body + link;
  }

  return renderNodesBlock(node.children);
}

function renderInlineNodes(nodes: BbNode[]): string {
  return nodes.map(renderInlineNode).join("");
}

function renderInlineNode(node: BbNode): string {
  if (node.type === "text") {
    return renderTextNode(node.text);
  }

  const children = renderInlineNodes(node.children);

  if (node.name === "B") return `<strong>${children}</strong>`;
  if (node.name === "I") return `<em>${children}</em>`;
  if (node.name === "U") return `<span class="eso-underline">${children}</span>`;
  if (node.name === "S") return `<s>${children}</s>`;
  if (transparentInlineTags.has(node.name)) return children;

  if (node.name === "COLOR") {
    const color = safeCssColor(node.attr);
    return color ? `<span class="eso-bb-color" style="color: ${escapeAttr(color)}">${children}</span>` : children;
  }

  if (node.name === "SIZE") {
    const level = sizeLevel(node.attr);
    if (level === 4) return `<span class="eso-size-large">${children}</span>`;
    if (level === 3) return `<span class="eso-size-medium">${children}</span>`;
    return children;
  }

  if (node.name === "URL") {
    const href = safeUrl(node.attr) ?? safeUrl(textContent(node.children).trim());
    if (!href) return children;
    const label = node.attr ? children || escapeHtml(href) : escapeHtml(href);
    return `<a href="${escapeAttr(href)}" target="_blank" rel="noreferrer noopener">${label}</a>`;
  }

  if (node.name === "IMG") {
    return renderImageNode(node);
  }

  if (node.name === "HR") {
    return '<hr class="eso-rule" />';
  }

  return children;
}

function renderImageNode(node: TagNode): string {
  const url = safeUrl(textContent(node.children).trim());
  if (!url) return "";
  return `
    <button class="bbcode-image-frame" type="button" data-lightbox-url="${escapeAttr(url)}" title="View larger image">
      <img class="bbcode-inline-image" src="${escapeAttr(url)}" alt="Addon description image" loading="lazy" />
    </button>
  `;
}

function renderTextNode(value: string): string {
  const normalized = value.replace(/\r\n?/g, "\n").replace(/\[BR\]/gi, "\n");
  const tagPattern = /\|c([0-9a-fA-F]{6}|[0-9a-fA-F]{8})([\s\S]*?)\|r/g;
  let output = "";
  let cursor = 0;
  let match: RegExpExecArray | null;

  while ((match = tagPattern.exec(normalized)) !== null) {
    output += escapeHtml(stripEsoPipeColorText(normalized.slice(cursor, match.index)));
    const color = match[1].length === 8 ? match[1].slice(2) : match[1];
    output += `<span class="eso-bb-color" style="color: #${escapeAttr(color)}">${escapeHtml(stripEsoPipeColorText(match[2]))}</span>`;
    cursor = match.index + match[0].length;
  }

  output += escapeHtml(stripEsoPipeColorText(normalized.slice(cursor)));
  return output;
}

function listItems(node: TagNode): TagNode[] {
  const items = node.children.filter((child): child is TagNode => child.type === "tag" && child.name === "LI");
  if (items.length > 0) return items;

  return textContent(node.children)
    .split(/\n+/)
    .map((text) => text.trim())
    .filter(Boolean)
    .map((text) => ({ type: "tag", name: "LI", attr: null, children: [{ type: "text", text }] }) satisfies TagNode);
}

function trimParagraphNodes(nodes: BbNode[]): BbNode[] {
  const copy = nodes.slice();
  while (copy.length > 0 && copy[0].type === "text" && copy[0].text.trim() === "") {
    copy.shift();
  }
  while (copy.length > 0) {
    const node = copy[copy.length - 1];
    if (node.type !== "text" || node.text.trim() !== "") break;
    copy.pop();
  }
  const first = copy[0];
  if (first?.type === "text") {
    copy[0] = { type: "text", text: first.text.replace(/^\s+/, "") };
  }
  const last = copy[copy.length - 1];
  if (last?.type === "text") {
    copy[copy.length - 1] = { type: "text", text: last.text.replace(/\s+$/, "") };
  }
  return copy;
}

function hasVisibleContent(nodes: BbNode[]): boolean {
  return nodes.some((node) => {
    if (node.type === "text") return node.text.trim().length > 0;
    if (node.name === "IMG") return Boolean(safeUrl(textContent(node.children).trim()));
    return hasVisibleContent(node.children);
  });
}

function standaloneHeadingLevel(nodes: BbNode[]): 3 | 4 | null {
  const meaningful = nodes.filter((node) => node.type !== "text" || node.text.trim().length > 0);
  if (meaningful.length !== 1) return null;
  return headingLevelFromNode(meaningful[0]);
}

function headingLevelFromNode(node: BbNode): 3 | 4 | null {
  if (node.type === "text") return null;
  if (node.name === "SIZE") return sizeLevel(node.attr);
  if (!styleWrapperTags.has(node.name)) return null;

  const meaningful = node.children.filter((child) => child.type !== "text" || child.text.trim().length > 0);
  return meaningful.length === 1 ? headingLevelFromNode(meaningful[0]) : null;
}

function sizeLevel(attr: string | null): 3 | 4 | null {
  const value = Number((attr ?? "").replace(/[^0-9.]/g, ""));
  if (value >= 4) return 4;
  if (value >= 3) return 3;
  return null;
}

function isBlockNode(node: TagNode) {
  return blockTags.has(node.name);
}

function containsBlockNode(node: TagNode): boolean {
  return node.children.some((child) => child.type === "tag" && (isBlockNode(child) || containsBlockNode(child)));
}

function textContent(nodes: BbNode[]): string {
  return nodes
    .map((node) => {
      if (node.type === "text") return node.text;
      return textContent(node.children);
    })
    .join("");
}

function stripEsoPipeColorText(value: string) {
  return value.replace(/\|c[0-9a-fA-F]{6,8}/g, "").replace(/\|r/g, "");
}

function safeUrl(value: string | null | undefined) {
  if (!value) return null;
  const trimmed = value.trim();
  try {
    const url = new URL(trimmed);
    if ((url.protocol === "http:" || url.protocol === "https:") && url.hostname) {
      return url.toString();
    }
  } catch {
    return null;
  }
  return null;
}

function safeCssColor(value: string | null) {
  if (!value) return null;
  const color = value.trim();
  const normalized = color.toLowerCase();
  if (safeNamedColors[normalized]) return safeNamedColors[normalized];
  if (/^#?[0-9a-f]{3}$/i.test(color) || /^#?[0-9a-f]{6}$/i.test(color)) {
    return color.startsWith("#") ? color : `#${color}`;
  }
  if (/^#?[0-9a-f]{8}$/i.test(color)) {
    return `#${color.replace(/^#/, "").slice(2)}`;
  }
  return null;
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
