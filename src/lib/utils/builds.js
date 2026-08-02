// Shared access to OpenTaiko build sources (GitHub releases and experimental builds).
import { fetch } from '@tauri-apps/plugin-http';
import { isVersionBelow } from './versions.js';

export const repoOwner = '0AuBSQ';//'OpenTaiko';
export const repoName = 'OpenTaiko';//OpenTaiko-Dev-Mirror';
// Branch builds of a version that is not released yet. Each entry is listed only while
// its `version` is still unreleased, so a branch drops off the list on its release day
// without touching instances already built from it. Add future InDev branches here.
const INDEV_BUILDS = [
    { branch: '0.6.1-skinning-characters', label: 'InDev 0.6.1', version: '0.6.1' }
];

// Fallbacks for instances recorded before the branch was stored on them
export const INDEV_BRANCH = INDEV_BUILDS[0].branch;
export const INDEV_LABEL = INDEV_BUILDS[0].label;

export const fetchLatestRelease = async () => {
    const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`);
    if (!response.ok) {
        throw new Error(`HTTP error status: ${response.status}`);
    }
    return await response.json();
};

// Experimental build sources: prereleases, plus any branch build whose target version
// has not been released yet. Listing a branch needs a known latest release, so nothing
// is offered when it cannot be fetched.
export const fetchExperimentalOptions = async (latestTag) => {
    const options = [];
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
            const releases = await response.json();
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

export const fetchBranchHeadSha = async (branch) => {
    const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/branches/${branch}`);
    if (!response.ok) {
        throw new Error(`HTTP error status: ${response.status}`);
    }
    return (await response.json()).commit?.sha ?? null;
};
