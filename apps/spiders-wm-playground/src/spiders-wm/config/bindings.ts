import * as commands from "@spiders-wm/sdk/commands";
import type { BindingsConfig } from "@spiders-wm/sdk/config";

export const bindings = {
  mod: "alt",
  entries: [
    { bind: ["mod", "Return"], command: commands.spawn("foot") },
    { bind: ["mod", "h"], command: commands.focus_dir("left") },
    { bind: ["mod", "j"], command: commands.focus_dir("down") },
    { bind: ["mod", "k"], command: commands.focus_dir("up") },
    { bind: ["mod", "l"], command: commands.focus_dir("right") },
    { bind: ["mod", "shift", "h"], command: commands.swap_dir("left") },
    { bind: ["mod", "shift", "j"], command: commands.swap_dir("down") },
    { bind: ["mod", "shift", "k"], command: commands.swap_dir("up") },
    { bind: ["mod", "shift", "l"], command: commands.swap_dir("right") },
    { bind: ["mod", "ctrl", "h"], command: commands.resize_dir("left") },
    { bind: ["mod", "ctrl", "j"], command: commands.resize_dir("down") },
    { bind: ["mod", "ctrl", "k"], command: commands.resize_dir("up") },
    { bind: ["mod", "ctrl", "l"], command: commands.resize_dir("right") },
    {
      bind: ["mod", "ctrl", "shift", "h"],
      command: commands.resize_tiled("left"),
    },
    {
      bind: ["mod", "ctrl", "shift", "j"],
      command: commands.resize_tiled("down"),
    },
    {
      bind: ["mod", "ctrl", "shift", "k"],
      command: commands.resize_tiled("up"),
    },
    {
      bind: ["mod", "ctrl", "shift", "l"],
      command: commands.resize_tiled("right"),
    },
    { bind: ["mod", "q"], command: commands.kill_client() },
    { bind: ["mod", "space"], command: commands.cycle_layout() },
    { bind: ["mod", "shift", "space"], command: commands.toggle_floating() },
    { bind: ["mod", "1"], command: commands.view_workspace(1) },
    { bind: ["mod", "2"], command: commands.view_workspace(2) },
    { bind: ["mod", "3"], command: commands.view_workspace(3) },
    { bind: ["mod", "4"], command: commands.view_workspace(4) },
    { bind: ["mod", "5"], command: commands.view_workspace(5) },
    { bind: ["mod", "shift", "1"], command: commands.assign_workspace(1) },
    { bind: ["mod", "shift", "2"], command: commands.assign_workspace(2) },
    { bind: ["mod", "shift", "3"], command: commands.assign_workspace(3) },
    { bind: ["mod", "shift", "4"], command: commands.assign_workspace(4) },
    { bind: ["mod", "shift", "5"], command: commands.assign_workspace(5) },
  ],
} satisfies BindingsConfig;
