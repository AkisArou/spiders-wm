import type { LayoutsConfig } from "spider-wm/config";

export const layouts: LayoutsConfig = {
  default: "master-stack",
  per_tag: [
    "master-stack",
    "primary-stack",
    "primary-stack",
    "primary-stack",
    "primary-stack",
    "primary-stack",
    "primary-stack",
    "primary-stack",
    "genymotion",
  ],
  per_monitor: {
    "eDP-1": "master-stack",
  },
}
