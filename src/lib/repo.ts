// Repo-related helpers shared by the updates list, update cards, and the
// dependency tree, so grouping order and badge colors always agree.

// Repos hosted on archlinux.org, in pacman.conf order. Must stay in sync with
// OFFICIAL_REPOS in src-tauri/src/checker.rs, which uses the same list to
// decide which packages get an archlinux.org package-page URL.
export const OFFICIAL_REPOS = [
  "core",
  "extra",
  "multilib",
  "core-testing",
  "extra-testing",
  "multilib-testing",
  "core-staging",
  "extra-staging",
  "multilib-staging",
  "kde-unstable",
  "gnome-unstable",
];

// Sentinel group name for packages whose repo couldn't be determined. Shared
// with the grouping in UpdatesDialog so the rank below can't desync from it.
export const UNKNOWN_REPO = "other";

// The repo name every AUR update carries — matches AUR_REPO in
// src-tauri/src/aur.rs. Shared so the "needs an AUR helper" grouping and the
// rank below key off the same string.
export const AUR_REPO = "aur";

// Rank tiers, derived from the list length so they can never collide with an
// official repo's index: official repos lead in the order above, then custom
// repos (e.g. paw, alphabetized by the caller), then AUR, then unknown last.
const TIER_CUSTOM = OFFICIAL_REPOS.length;
const TIER_AUR = TIER_CUSTOM + 1;
const TIER_UNKNOWN = TIER_CUSTOM + 2;

export function repoRank(repo: string): number {
  const i = OFFICIAL_REPOS.indexOf(repo);
  if (i !== -1) return i;
  if (repo === AUR_REPO) return TIER_AUR;
  if (repo === UNKNOWN_REPO) return TIER_UNKNOWN;
  return TIER_CUSTOM;
}

// Build the var() reference for a repo's theme color. Repo names may contain
// characters illegal in a CSS custom-property name (e.g. "."), which would
// void the whole style declaration — sanitize here so every call site keys
// off the same variable name.
export function repoColorVar(repo: string, fallbackVar: string): string {
  const slug = repo.replace(/[^a-zA-Z0-9_-]/g, "-");
  return `var(--ys-repo-${slug}, var(${fallbackVar}))`;
}
