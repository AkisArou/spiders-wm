import type { LayoutDiagnostic } from "./layout";

export const ALLOWED_LAYOUT_PROPERTIES = [
  "align-content",
  "align-items",
  "align-self",
  "aspect-ratio",
  "background",
  "background-color",
  "border",
  "border-bottom",
  "border-bottom-color",
  "border-bottom-style",
  "border-bottom-width",
  "border-color",
  "border-left",
  "border-left-color",
  "border-left-style",
  "border-left-width",
  "border-radius",
  "border-right",
  "border-right-color",
  "border-right-style",
  "border-right-width",
  "border-style",
  "border-top",
  "border-top-color",
  "border-top-style",
  "border-top-width",
  "border-width",
  "box-shadow",
  "box-sizing",
  "color",
  "column-gap",
  "display",
  "flex-basis",
  "flex-direction",
  "flex-grow",
  "flex-shrink",
  "flex-wrap",
  "font-family",
  "font-size",
  "font-weight",
  "gap",
  "grid-auto-columns",
  "grid-auto-flow",
  "grid-auto-rows",
  "grid-column",
  "grid-column-end",
  "grid-column-start",
  "grid-row",
  "grid-row-end",
  "grid-row-start",
  "grid-template-areas",
  "grid-template-columns",
  "grid-template-rows",
  "height",
  "inset",
  "justify-content",
  "justify-items",
  "justify-self",
  "left",
  "letter-spacing",
  "margin",
  "margin-bottom",
  "margin-left",
  "margin-right",
  "margin-top",
  "max-height",
  "max-width",
  "min-height",
  "min-width",
  "opacity",
  "overflow",
  "overflow-x",
  "overflow-y",
  "padding",
  "padding-bottom",
  "padding-left",
  "padding-right",
  "padding-top",
  "position",
  "right",
  "row-gap",
  "text-align",
  "text-transform",
  "top",
  "transform",
  "width",
] as const;

const allowedPropertySet: ReadonlySet<string> = new Set(
  ALLOWED_LAYOUT_PROPERTIES,
);
const allowedPseudoClasses = [
  "focused",
  "floating",
  "fullscreen",
  "urgent",
  "enter-from-left",
  "enter-from-right",
  "exit-to-left",
  "exit-to-right",
].join("|");
const allowedMatchKeys = [
  "app_id",
  "title",
  "class",
  "instance",
  "role",
  "shell",
  "window_type",
].join("|");

const selectorPatterns = [
  new RegExp(
    `^(workspace|group|window)(?::(${allowedPseudoClasses}))*(::titlebar)?$`,
  ),
  new RegExp(`^#[A-Za-z_][\\w-]*(?::(${allowedPseudoClasses}))*(::titlebar)?$`),
  new RegExp(
    `^\\.[A-Za-z_][\\w-]*(?::(${allowedPseudoClasses}))*(::titlebar)?$`,
  ),
  new RegExp(
    `^window\\[(${allowedMatchKeys})="[^"]*"\\](?::(${allowedPseudoClasses}))*(::titlebar)?$`,
  ),
];

export function validateLayoutStylesheet(source: string): LayoutDiagnostic[] {
  const diagnostics: LayoutDiagnostic[] = [];
  const cleanedSource = source.replace(/\/\*[\s\S]*?\*\//g, "");
  const trimmed = cleanedSource.trim();

  if (!trimmed) {
    return diagnostics;
  }

  if (trimmed.includes("@")) {
    diagnostics.push({
      source: "css",
      level: "warning",
      message:
        "At-rules are not validated by the playground subset checker yet.",
    });
  }

  const blockPattern = /([^{}]+)\{([^{}]*)\}/g;
  let matchedAnyBlock = false;

  for (const match of cleanedSource.matchAll(blockPattern)) {
    matchedAnyBlock = true;
    const selectorSource = match[1]?.trim() ?? "";
    const declarationSource = match[2] ?? "";

    for (const selector of selectorSource
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean)) {
      if (!selectorPatterns.some((pattern) => pattern.test(selector))) {
        diagnostics.push({
          source: "css",
          level: "error",
          path: selector,
          message: `Unsupported layout selector: ${selector}`,
        });
      }
    }

    for (const declaration of declarationSource
      .split(";")
      .map((value) => value.trim())
      .filter(Boolean)) {
      const separatorIndex = declaration.indexOf(":");
      if (separatorIndex < 0) {
        diagnostics.push({
          source: "css",
          level: "error",
          message: `Invalid declaration syntax: ${declaration}`,
          path: selectorSource,
        });
        continue;
      }

      const property = declaration
        .slice(0, separatorIndex)
        .trim()
        .toLowerCase();
      if (!allowedPropertySet.has(property)) {
        diagnostics.push({
          source: "css",
          level: "error",
          path: selectorSource,
          message: `Unsupported layout property: ${property}`,
        });
      }
    }
  }

  if (!matchedAnyBlock) {
    diagnostics.push({
      source: "css",
      level: "warning",
      message:
        "The stylesheet did not contain any simple rules the playground validator could parse.",
    });
  }

  return diagnostics;
}
