import { events } from "@spiders-wm/sdk/api";
import type { SpiderWMConfig } from "@spiders-wm/sdk/config";

import { bindings } from "./config/bindings.ts";
import { inputs } from "./config/inputs.ts";
import { layouts } from "./config/layouts.ts";

events.on("config-reloaded", () => {});

export default {
  workspaces: ["1", "2", "3", "4", "5"],

  titlebars: [
    {
      class: "default-titlebar",
      children: [
        {
          type: "group",
          class: "titlebar-left",
          children: [],
        },
        {
          type: "group",
          class: "titlebar-center",
          children: [{ type: "windowTitle", class: "window-title" }],
        },
        {
          type: "group",
          class: "titlebar-right",
          children: [
            {
              type: "button",
              class: "close-button",
              onClick: { action: "close" },
              children: [
                {
                  type: "icon",
                  class: "close-icon",
                  children: [
                    {
                      type: "svg",
                      viewBox: "0 0 16 16",
                      children: [
                        {
                          type: "path",
                          d: "M3 4.25 L4.25 3 L8 6.75 L11.75 3 L13 4.25 L9.25 8 L13 11.75 L11.75 13 L8 9.25 L4.25 13 L3 11.75 L6.75 8 Z",
                        },
                      ],
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
    },
  ],

  options: {
    sloppyfocus: true,
    attach: "after",
  },

  inputs,
  layouts,
  bindings,
} satisfies SpiderWMConfig;
