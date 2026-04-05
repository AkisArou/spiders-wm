const flattenChildren = (input, out) => {
  for (const child of input) {
    if (Array.isArray(child)) {
      flattenChildren(child, out);
      continue;
    }
    if (child === false || child === null || child === undefined) {
      continue;
    }
    out.push(child);
  }
};

const createTitlebarNode = (type, props) => {
  const nextProps = props || {};
  const children = [];
  if (Object.prototype.hasOwnProperty.call(nextProps, "children")) {
    flattenChildren([nextProps.children], children);
  }
  const runtimeProps = { ...nextProps };
  delete runtimeProps.children;
  return {
    type,
    props: runtimeProps,
    children,
  };
};

export const titlebar = {
  group: (props) => createTitlebarNode("titlebar.group", props),
  windowTitle: (props) => createTitlebarNode("titlebar.windowTitle", props),
  workspaceName: (props) => createTitlebarNode("titlebar.workspaceName", props),
  text: (props) => createTitlebarNode("titlebar.text", props),
  badge: (props) => createTitlebarNode("titlebar.badge", props),
  button: (props) => createTitlebarNode("titlebar.button", props),
  icon: (props) => createTitlebarNode("titlebar.icon", props),
};
