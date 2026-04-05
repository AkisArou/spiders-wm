import type { SpiderWMConfig } from "@spiders-wm/sdk/config";
import { events } from "@spiders-wm/sdk/api";
import { titlebar } from "@spiders-wm/sdk/titlebar";

import { bindings } from "./config/bindings.ts";
import { inputs } from "./config/inputs.ts";
import { layouts } from "./config/layouts.ts";

events.on("config-reloaded", () => {});

export default {
  workspaces: ["1", "2", "3", "4", "5", "6", "7", "8", "9"],

  titlebars: [
    <titlebar class="default-titlebar">
      <titlebar.group class="left">
        <titlebar.workspaceName class="workspace-name" />
      </titlebar.group>

      <titlebar.group class="center">
        <titlebar.windowTitle class="window-title" />
      </titlebar.group>

      <titlebar.group class="right">
        <titlebar.button class="close-button" onClick={{ action: "close" }}>
          <titlebar.text>x</titlebar.text>
        </titlebar.button>
      </titlebar.group>
    </titlebar>,
  ],

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
