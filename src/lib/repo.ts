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

// Sort key for repo groups: official repos lead in the order above; custom
// repos (e.g. paw) follow alphabetically; AUR is last; "other" (packages with
// no known repo) sinks below AUR.
export function repoRank(repo: string): number {
  const i = OFFICIAL_REPOS.indexOf(repo);
  if (i !== -1) return i;
  if (repo === "aur") return 200;
  if (repo === "other") return 300;
  return 100;
}

// Build the var() reference for a repo's theme color. Repo names may contain
// characters illegal in a CSS custom-property name (e.g. "."), which would
// void the whole style declaration — sanitize here so every call site keys
// off the same variable name.
export function repoColorVar(repo: string, fallbackVar: string): string {
  const slug = repo.replace(/[^a-zA-Z0-9_-]/g, "-");
  return `var(--ys-repo-${slug}, var(${fallbackVar}))`;
}
