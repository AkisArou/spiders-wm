import type { InputsConfig } from "@spiders-wm/sdk/config";

export const inputs: InputsConfig = {
  "type:keyboard": {
    xkb_layout: "us",
    repeat_rate: 50,
    repeat_delay: 275,
  },
  "type:touchpad": {
    natural_scroll: false,
    tap: true,
    accel_profile: "adaptive",
  },
  "type:pointer": {
    accel_profile: "flat",
  },
};
