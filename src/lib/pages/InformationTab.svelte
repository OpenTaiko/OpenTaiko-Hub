<script>
    // Dependencies
    import { onMount } from 'svelte';
    import { TabGroup, Tab } from '@skeletonlabs/skeleton';
    import { fetch } from "@tauri-apps/plugin-http";
    import { marked } from 'marked';

    let currentInfo = 0;

    // Information
    const changelogUrl = 'https://raw.githubusercontent.com/0auBSQ/OpenTaiko/main/CHANGELOG.md';
    let changelogContent = '';
    const hubChangelogUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Hub/main/CHANGELOG.md';
    let hubChangelogContent = '';
    const creditsUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Hub/main/CREDITS.md';
    let creditsContent = '';

    const renderer = new marked.Renderer();
    renderer.link = function(href, title, text) {
        let link = marked.Renderer.prototype.link.call(this, href, title, text);
        return link.replace("<a","<a target='_blank' class='text-blue-600 underline'");
    };
    marked.setOptions({
        renderer: renderer
    })

    const updateChangeLogs = async () => {
        try {
            const response = await fetch(changelogUrl);
        if (response.ok) {
            const text = await response.text();
            changelogContent = marked(text);
        } else {
            changelogContent = '<p>Failed to load changelog.</p>';
        }
        } catch (error) {
            changelogContent = `<p>Error: ${error.message}</p>`;
        }
    }

    const updateHubChangeLogs = async () => {
        try {
            const response = await fetch(hubChangelogUrl);
        if (response.ok) {
            const text = await response.text();
            hubChangelogContent = marked(text);
        } else {
            hubChangelogContent = '<p>Failed to load changelog.</p>';
        }
        } catch (error) {
            hubChangelogContent = `<p>Error: ${error.message}</p>`;
        }
    }

    const updateCredits = async () => {
        try {
            const response = await fetch(creditsUrl);
        if (response.ok) {
            const text = await response.text();
            creditsContent = marked(text);
        } else {
            creditsContent = '<p>Failed to load credits information.</p>';
        }
        } catch (error) {
            creditsContent = `<p>Error: ${error.message}</p>`;
        }
    }

    onMount(async () => {
        updateChangeLogs();
        updateHubChangeLogs();
        updateCredits();
    });

</script>

<TabGroup 
	justify="justify-center"
	active="variant-filled-primary"
	hover="hover:variant-soft-primary"
	flex="flex-1 lg:flex-none"
	rounded=""
	border=""
	class="bg-surface-100-800-token w-full"
	>
	<Tab bind:group={currentInfo} name="tab1" value={0}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>Changelogs (Game)</span>
	</Tab>
	<Tab bind:group={currentInfo} name="tab2" value={1}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>Changelogs (Hub)</span>
	</Tab>
	<Tab bind:group={currentInfo} name="tab3" value={2}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>Documentation</span>
	</Tab>
	<Tab bind:group={currentInfo} name="tab4" value={3}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>Troubleshooting</span>
	</Tab>
	<Tab bind:group={currentInfo} name="tab5" value={4}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>Credits</span>
	</Tab>
	<!-- ... -->
</TabGroup>

<!-- OpenTaiko Changelogs -->
{#if currentInfo === 0}
    <div class="content">
        {@html changelogContent}
    </div>
{/if}

<!-- OpenTaiko Hub Changelogs -->
{#if currentInfo === 1}
    <div class="content">
        {@html hubChangelogContent}
    </div>
{/if}

<!-- Documentation -->
{#if currentInfo === 2}
    <iframe src="https://opentaiko.github.io/OpTk-Documentation/" title="OpenTaiko Documentation" width="100%"  style="background-color:white;height:calc(100% - 100px)"></iframe>
{/if}

<!-- Troubleshooting -->
{#if currentInfo === 3}
    <div class="content">
        <h2>Support / Bug report</h2>
        <p>If you encounter an issue or are searching for support about OpenTaiko, please refer to the following links:</p>
        <a href="https://discord.gg/5xfpGuwASU" target='_blank' class='text-blue-600 underline'>OpenTaiko Discord's #support channel</a>
        <br />
        <a href="https://github.com/0auBSQ/OpenTaiko/issues" target='_blank' class='text-blue-600 underline'>Github issues section</a>
        <p>Note: Be sure to do your own research before asking as the issue might have been treated before, be sure to check Discord's #often_asked_question channel too.</p>
        <h2>Skin / Asset versioning</h2>
        <p>All the numbers up to the minor update number (ie. v[0.6.0].2) need to match between a skin and a game version in order to be compatible.</p>
        <p>It is not mandatory for some asset (character/puchichara) updates as they do not necessarily follow the strict versioning patterns skins do.</p>
        <p>The revision number (ie. v0.6.0.[2]) is incremented at each game/skin update that do not imply mandatory changes and do not need to match.</p>
        <p>Example: a skin with a 0.6.1.3 version number will work with any OpenTaiko version from 0.6.1.0 and will stop to be compatible from 0.6.2.0.</p>
        <p>This section will be completed in future OpenTaiko Hub updates depending on found issues.</p>
    </div>
{/if}

<!-- Credits -->
{#if currentInfo === 4}
    <div class="content">
        {@html creditsContent}
    </div>
{/if}

<style>
.content {
    @apply bg-surface-100-800-token;
    @apply card w-full;
    padding: 1rem;
}

</style>