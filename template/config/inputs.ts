import type { InputsConfig } from "spider-wm/config";

export const inputs = {
  "type:keyboard": {
    repeat_delay: 220,
    repeat_rate: 33,
  },
  "type:touchpad": {
    natural_scroll: true,
    tap: true,
  },
} satisfies InputsConfig;
