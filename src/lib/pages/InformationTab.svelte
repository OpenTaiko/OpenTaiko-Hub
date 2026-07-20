<script>
    // Dependencies
    import { onMount } from 'svelte';
    import { TabGroup, Tab } from '@skeletonlabs/skeleton';
    import { Accordion, AccordionItem } from '@skeletonlabs/skeleton';
    import { fetch } from "@tauri-apps/plugin-http";
    import { marked } from 'marked';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

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
        renderer: renderer,
        gfm: true,
        breaks: true
    })

    const updateChangeLogs = async () => {
        try {
            const response = await fetch(changelogUrl);
        if (response.ok) {
            const text = await response.text();
            changelogContent = marked(text);
        } else {
            changelogContent = `<p>${get(_)('info.error.changelog')}</p>`;
        }
        } catch (error) {
            changelogContent = `<p>${get(_)('info.error.generic', { values: { error: error.message } })}</p>`;
        }
    }

    const updateHubChangeLogs = async () => {
        try {
            const response = await fetch(hubChangelogUrl);
        if (response.ok) {
            const text = await response.text();
            hubChangelogContent = marked(text);
        } else {
            hubChangelogContent = `<p>${get(_)('info.error.changelog')}</p>`;
        }
        } catch (error) {
            hubChangelogContent = `<p>${get(_)('info.error.generic', { values: { error: error.message } })}</p>`;
        }
    }

    const updateCredits = async () => {
        try {
            const response = await fetch(creditsUrl);
        if (response.ok) {
            const text = await response.text();
            creditsContent = marked(text);
        } else {
            creditsContent = `<p>${get(_)('info.error.credits')}</p>`;
        }
        } catch (error) {
            creditsContent = `<p>${get(_)('info.error.generic', { values: { error: error.message } })}</p>`;
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
		<span>{$_('info.tab.changelog_game')}</span>
	</Tab>
	<Tab bind:group={currentInfo} name="tab2" value={1}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>{$_('info.tab.changelog_hub')}</span>
	</Tab>
	<Tab bind:group={currentInfo} name="tab4" value={3}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>{$_('info.tab.troubleshooting')}</span>
	</Tab>
	<Tab bind:group={currentInfo} name="tab5" value={4}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-lines"></i></svelte:fragment>
		<span>{$_('info.tab.credits')}</span>
	</Tab>
	<!-- ... -->
</TabGroup>

<!-- OpenTaiko Changelogs or OpenTaiko Hub Changelogs -->
{#if currentInfo === 0 || currentInfo === 1}
    <div class="content">
        <Accordion class="card rounded-container-token">
            <AccordionItem>
                <svelte:fragment slot="summary"><b>{$_('info.legend.title')}</b></svelte:fragment>
                <svelte:fragment slot="content">
                    <h2><b>[Feat]</b></h2>
                    <p>{$_('info.legend.feat')}</p>

                    <h2><b>[Enhance(ment)]</b></h2>
                    <p>{$_('info.legend.enhancement')}</p>

                    <h2><b>[Fix/BugFix]</b></h2>
                    <p>{$_('info.legend.fix')}
                    <br><span class="smalltext"><b>{$_('info.legend.fix_note')}</b></span></p>

                    <h2><b>[Chore]</b></h2>
                    <p>{$_('info.legend.chore')}</p>

                    <h2><b>[i18n]</b></h2>
                    <p>{$_('info.legend.i18n')}</p>

                    {#if currentInfo === 1}
                        <h2><b>[Theme]</b></h2>
                        <p>{$_('info.legend.theme')}</p>
                    {/if}
                </svelte:fragment>
            </AccordionItem>
        </Accordion>

        <hr class="my-3">

        {#if currentInfo === 0}
            {@html changelogContent}
        {:else if currentInfo === 1}
            {@html hubChangelogContent}
        {/if}
    </div>
{/if}

<!-- Documentation moved to per-build Docs (Home tab); no online documentation here anymore -->


<!-- Troubleshooting -->
{#if currentInfo === 3}
    <div class="content">
        <h1>{$_('info.troubleshoot.title')}</h1>
        <p>{$_('info.troubleshoot.intro')}</p>
        <a href="https://discord.gg/5xfpGuwASU" target='_blank' class='text-blue-600 underline'>{$_('info.troubleshoot.discord')}</a>
        <br />
        <a href="https://github.com/0auBSQ/OpenTaiko/issues" target='_blank' class='text-blue-600 underline'>{$_('info.troubleshoot.github')}</a>
        <p>{$_('info.troubleshoot.note')}</p>

        <hr class="m-4">

        <h1>{$_('info.versioning.title')}</h1>
        <p>{$_('info.versioning.compat')}</p>
        <p>{$_('info.versioning.optional')}</p>
        <p>{$_('info.versioning.revision')}</p>
        <p>{$_('info.versioning.example')}</p>
        <p>{$_('info.versioning.wip')}</p>
    </div>
{/if}

<!-- Credits -->
{#if currentInfo === 4}
    <div class="content">
        {@html creditsContent}
    </div>
{/if}

<style>
    .content {@apply card w-full bg-surface-100-800-token p-4;}
</style>
