import type { SpiderWMConfig } from "spider-wm/config";
import { events } from "spider-wm/api";

import { bindings } from "./config/bindings";
import { inputs } from "./config/inputs";
import { layouts } from "./config/layouts";

events.on("config-reloaded", () => {});

export default {
  tags: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],

  options: {
    sloppyfocus: true,
  },

  inputs,
  layouts,
  bindings,
} satisfies SpiderWMConfig;
