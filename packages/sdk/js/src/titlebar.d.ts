export interface TitlebarWhen {
  workspace?: string;
  slot?: string;
  appId?: string;
  title?: string;
  floating?: boolean;
  fullscreen?: boolean;
}

export interface TitlebarBaseProps {
  class?: string;
}

export interface TitlebarComponentProps {
  children?: TitlebarRenderable;
}

export interface TitlebarProps extends TitlebarBaseProps, TitlebarComponentProps {
  when?: TitlebarWhen;
  disabled?: boolean;
}

export interface TitlebarGroupProps extends TitlebarBaseProps, TitlebarComponentProps {}

export interface TitlebarWindowTitleProps extends TitlebarBaseProps {
  fallback?: string;
}

export interface TitlebarWorkspaceNameProps extends TitlebarBaseProps {}

export interface TitlebarTextProps extends TitlebarBaseProps, TitlebarComponentProps {}

export interface TitlebarBadgeProps extends TitlebarBaseProps, TitlebarComponentProps {}

export interface TitlebarButtonProps extends TitlebarBaseProps, TitlebarComponentProps {
  onClick?: unknown;
}

export interface TitlebarIconProps extends TitlebarBaseProps, TitlebarComponentProps {
  asset?: string;
}

export declare const titlebar: {
  group(props: TitlebarGroupProps): TitlebarGroupNode;
  windowTitle(props: TitlebarWindowTitleProps): TitlebarWindowTitleNode;
  workspaceName(props: TitlebarWorkspaceNameProps): TitlebarWorkspaceNameNode;
  text(props: TitlebarTextProps): TitlebarTextNode;
  badge(props: TitlebarBadgeProps): TitlebarBadgeNode;
  button(props: TitlebarButtonProps): TitlebarButtonNode;
  icon(props: TitlebarIconProps): TitlebarIconNode;
};

export interface TitlebarNodeBase<Props> {
  props?: Props;
  children?: TitlebarChild[];
}

export interface TitlebarNode extends TitlebarNodeBase<TitlebarProps> {
  type: "titlebar";
}

export interface TitlebarGroupNode extends TitlebarNodeBase<TitlebarGroupProps> {
  type: "titlebar.group";
}

export interface TitlebarWindowTitleNode
  extends TitlebarNodeBase<TitlebarWindowTitleProps> {
  type: "titlebar.windowTitle";
}

export interface TitlebarWorkspaceNameNode
  extends TitlebarNodeBase<TitlebarWorkspaceNameProps> {
  type: "titlebar.workspaceName";
}

export interface TitlebarTextNode extends TitlebarNodeBase<TitlebarTextProps> {
  type: "titlebar.text";
}

export interface TitlebarBadgeNode extends TitlebarNodeBase<TitlebarBadgeProps> {
  type: "titlebar.badge";
}

export interface TitlebarButtonNode extends TitlebarNodeBase<TitlebarButtonProps> {
  type: "titlebar.button";
}

export interface TitlebarIconNode extends TitlebarNodeBase<TitlebarIconProps> {
  type: "titlebar.icon";
}

export type TitlebarElementNode =
  | TitlebarNode
  | TitlebarGroupNode
  | TitlebarWindowTitleNode
  | TitlebarWorkspaceNameNode
  | TitlebarTextNode
  | TitlebarBadgeNode
  | TitlebarButtonNode
  | TitlebarIconNode;

export type TitlebarRenderable =
  | TitlebarElementNode
  | string
  | number
  | boolean
  | null
  | undefined
  | TitlebarRenderable[];

export type TitlebarChild = TitlebarElementNode | string | number | null;
