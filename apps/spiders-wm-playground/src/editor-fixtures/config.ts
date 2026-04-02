import masterStack from "../layouts/master-stack/index";

export default {
  startupWorkspace: "1:dev",
  workspaces: ["1:dev", "2:web", "3:chat"],
  layout: masterStack,
  monitorRules: {
    "DP-1": {
      workspace: "1:dev",
      scale: 1,
    },
  },
};