import type { SpiderWMConfig } from "@spiders-wm/sdk/config";
// import { events, query, wm } from "@spiders-wm/sdk/api";

import { bindings } from "./config/bindings.ts";
import { inputs } from "./config/inputs.ts";
import { layouts } from "./config/layouts.ts";
import { defaultTitlebar } from "./config/titlebar.tsx";

// events.on("config-reloaded", () => {});
// events.once("window-created", ({ window }) => {
//   if (window?.appId === query.getFocusedWindow()?.appId && window?.appId === "foot") {
//     wm.toggleFloating();
//   }
// });

export default {
  workspaces: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],
  titlebars: [defaultTitlebar],

  options: {
    sloppyfocus: true,
    attach: "after",
  },

  inputs,
  layouts,
  rules: [],
  bindings,
} satisfies SpiderWMConfig;
