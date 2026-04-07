import type { SpiderWMConfig } from "@spiders-wm/sdk/config";
import { events } from "@spiders-wm/sdk/api";

import { bindings } from "./config/bindings.ts";
import { inputs } from "./config/inputs.ts";
import { layouts } from "./config/layouts.ts";

events.on("config-reloaded", () => {});

export default {
  workspaces: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],

  options: {
    sloppyfocus: true,
  },

  inputs,
  layouts,
  bindings,
} satisfies SpiderWMConfig;
