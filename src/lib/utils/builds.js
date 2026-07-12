// Shared access to OpenTaiko build sources (GitHub releases and experimental builds).
import { fetch } from '@tauri-apps/plugin-http';
import { isVersionBelow } from './versions.js';

export const repoOwner = '0AuBSQ';//'OpenTaiko';
export const repoName = 'OpenTaiko';//OpenTaiko-Dev-Mirror';
export const INDEV_BRANCH = '0.6.1-skinning-characters';
export const INDEV_LABEL = 'InDev 0.6.1';

export const fetchLatestRelease = async () => {
    const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}/releases/latest`);
    if (!response.ok) {
        throw new Error(`HTTP error status: ${response.status}`);
    }
    return await response.json();
};

// Experimental build sources: prereleases, plus the "InDev 0.6.1" branch build while
// the latest stable release is older than 0.6.1.
export const fetchExperimentalOptions = async (latestTag) => {
    const options = [];
    if (latestTag && isVersionBelow(latestTag, '0.6.1')) {
        options.push({ kind: 'indev', branch: INDEV_BRANCH, label: INDEV_LABEL });
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
