const VALID_PLATFORMS = new Set(["wayland", "xorg", "web"]);

function resolvePlatform() {
  const value = globalThis.__SPIDERS_WM_PLATFORM;
  if (VALID_PLATFORMS.has(value)) {
    return value;
  }

  throw new Error(
    "spiders-wm platform is not initialized; expected one of wayland, xorg, or web"
  );
}

export const platform = resolvePlatform();

export function platformMatch(branches) {
  const handler = branches?.[platform] ?? branches?.default;
  if (typeof handler !== "function") {
    throw new Error(
      `spiders-wm platformMatch() has no branch for platform \"${platform}\" and no default fallback`
    );
  }

  return handler();
}
