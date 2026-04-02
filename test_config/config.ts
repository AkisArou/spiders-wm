import type { SpiderWMConfig } from "@spiders-wm/sdk/config";
// import { events, query, wm } from "@spiders-wm/sdk/api";

import { bindings } from "./config/bindings";
import { inputs } from "./config/inputs";
import { layouts } from "./config/layouts";

// events.on("config-reloaded", () => {});
// events.once("window-created", ({ window }) => {
//   if (window?.appId === query.getFocusedWindow()?.appId && window?.appId === "foot") {
//     wm.toggleFloating();
//   }
// });

export default {
  workspaces: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],

  options: {
    sloppyfocus: true,
    // titlebar_font: {
    //   regular_path: "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    //   bold_path: "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
    // },
  },

  inputs,
  layouts,

  rules: [],

  bindings,
} satisfies SpiderWMConfig;
