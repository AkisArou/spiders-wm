import type { InputsConfig } from "@spiders-wm/sdk/config";

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
