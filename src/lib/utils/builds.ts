// Shared access to OpenTaiko build sources (GitHub releases and experimental builds).
import { fetch } from '@tauri-apps/plugin-http';
import { isVersionBelow } from './versions';
import type { BuildOption, GitHubRelease } from '$lib/types';

export const repoOwner = '0AuBSQ';//'OpenTaiko';
export const repoName = 'OpenTaiko';//OpenTaiko-Dev-Mirror';

interface IndevBuild {
    branch: string;
    label: string;
    /** The version this branch will become; the branch is offered until it is released. */
    version: string;
}

// Branch builds of a version that is not released yet. Each entry is listed only while
// its `version` is still unreleased, so a branch drops off the list on its release day
// without touching instances already built from it. Add future InDev branches here.
const INDEV_BUILDS: IndevBuild[] = [
    { branch: '0.6.1-skinning-characters', label: 'InDev 0.6.1', version: '0.6.1' }
];

// Fallbacks for instances recorded before the branch was stored on them
export const INDEV_BRANCH = INDEV_BUILDS[0].branch;
export const INDEV_LABEL = INDEV_BUILDS[0].label;

export const fetchLatestRelease = async (): Promise<GitHubRelease> => {
    const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`);
    if (!response.ok) {
        throw new Error(`HTTP error status: ${response.status}`);
    }
    return (await response.json()) as GitHubRelease;
};

// Experimental build sources: prereleases, plus any branch build whose target version
// has not been released yet. Listing a branch needs a known latest release, so nothing
// is offered when it cannot be fetched.
export const fetchExperimentalOptions = async (latestTag: string | null): Promise<BuildOption[]> => {
    const options: BuildOption[] = [];
    if (latestTag) {
        for (const build of INDEV_BUILDS) {
            if (isVersionBelow(latestTag, build.version)) {
                options.push({ kind: 'indev', branch: build.branch, label: build.label });
            }
        }
    }
    try {
        const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases?per_page=15`);
        if (response.ok) {
            const releases = (await response.json()) as GitHubRelease[];
            const prerelease = releases.find((release) => release.prerelease && !release.draft);
            if (prerelease) {
                options.push({ kind: 'prerelease', tag: prerelease.tag_name, label: prerelease.tag_name });
            }
        }
    } catch (error) {
        console.error('Failed to list prereleases:', error);
    }
    return options;
};

export const fetchReleaseByTag = async (tag: string): Promise<GitHubRelease> => {
    const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/tags/${encodeURIComponent(tag)}`);
    if (!response.ok) {
        throw new Error(`HTTP error status: ${response.status}`);
    }
    return (await response.json()) as GitHubRelease;
};

export const fetchBranchHeadSha = async (branch: string): Promise<string | null> => {
    const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/branches/${branch}`);
    if (!response.ok) {
        throw new Error(`HTTP error status: ${response.status}`);
    }
    const data = (await response.json()) as { commit?: { sha?: string } };
    return data.commit?.sha ?? null;
};
