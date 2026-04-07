import { titlebar } from "@spiders-wm/sdk/titlebar";

export const defaultTitlebar = (
  <titlebar class="default-titlebar">
    <titlebar.group class="titlebar-left"></titlebar.group>

    <titlebar.group class="titlebar-center">
      <titlebar.windowTitle class="window-title" />
    </titlebar.group>

    <titlebar.group class="titlebar-right">
      <titlebar.button class="close-button" onClick={{ action: "close" }}>
        <titlebar.icon class="close-icon">
          <svg viewBox="0 0 16 16">
            <path d="M3 4.25 L4.25 3 L8 6.75 L11.75 3 L13 4.25 L9.25 8 L13 11.75 L11.75 13 L8 9.25 L4.25 13 L3 11.75 L6.75 8 Z" />
          </svg>
        </titlebar.icon>
      </titlebar.button>
    </titlebar.group>
  </titlebar>
);
