import { events } from "@spiders-wm/sdk/api";
import type { SpiderWMConfig } from "@spiders-wm/sdk/config";

import { bindings } from "./config/bindings.ts";
import { inputs } from "./config/inputs.ts";
import { layouts } from "./config/layouts.ts";

events.on("config-reloaded", () => {});

export default {
  workspaces: ["1", "2", "3", "4", "5"],

  options: {
    sloppyfocus: true,
    attach: "after",
  },

  inputs,
  layouts,
  bindings,
} satisfies SpiderWMConfig;
