import type { InputsConfig } from "spider-wm/config";

export const inputs: InputsConfig = {
  "type:keyboard": {
    xkb_layout: "us,gr",
    xkb_model: "pc105+inet",
    xkb_options: "grp:win_space_toggle",
    repeat_delay: 220,
    repeat_rate: 33,
  },
  "type:touchpad": {
    natural_scroll: true,
    tap: true,
    accel_profile: "adaptive",
  },
  "type:pointer": {
    accel_profile: "flat",
  },
}
