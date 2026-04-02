import type { SpiderWMConfig } from "@spiders-wm/sdk/config";
import { events } from "@spiders-wm/sdk/api";

import { bindings } from "./config/bindings";
import { inputs } from "./config/inputs";
import { layouts } from "./config/layouts";

events.on("config-reloaded", () => {});

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
  bindings,
} satisfies SpiderWMConfig;
