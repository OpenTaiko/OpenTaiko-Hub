// Every repository the Hub reads from or installs from, in one place. The game is
// developed under its author's account; the Hub, the soundtrack and the skins live in
// the OpenTaiko organisation.
//
// The backend downloads from these repositories only (REPOSITORIES in
// src-tauri/src/download.rs): one added or moved has to be allowed there too.

export interface Repo {
    owner: string;
    name: string;
    branch: string;
}

export const GAME_REPO: Repo = { owner: '0AuBSQ', name: 'OpenTaiko', branch: 'main' };
export const HUB_REPO: Repo = { owner: 'OpenTaiko', name: 'OpenTaiko-Hub', branch: 'main' };
export const SOUNDTRACK_REPO: Repo = { owner: 'OpenTaiko', name: 'OpenTaiko-Soundtrack', branch: 'main' };
export const SKINS_REPO: Repo = { owner: 'OpenTaiko', name: 'OpenTaiko-Skins', branch: 'main' };

/** The repository's GitHub API root, `…/repos/<owner>/<name>`. */
export const apiUrl = (repo: Repo): string => `https://api.github.com/repos/${repo.owner}/${repo.name}`;

/** A file of the repository's default branch, served as is. */
export const rawUrl = (repo: Repo, path = ''): string =>
    `https://raw.githubusercontent.com/${repo.owner}/${repo.name}/${repo.branch}${path ? `/${path}` : ''}`;

/** The repository's page, or a page below it (`releases/latest`, `issues`). */
export const pageUrl = (repo: Repo, path = ''): string =>
    `https://github.com/${repo.owner}/${repo.name}${path ? `/${path}` : ''}`;

/** A source archive of one commit. */
export const sourceZipUrl = (repo: Repo, sha: string): string =>
    `https://codeload.github.com/${repo.owner}/${repo.name}/zip/${sha}`;
