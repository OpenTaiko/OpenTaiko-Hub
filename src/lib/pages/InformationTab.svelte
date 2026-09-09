<script lang="ts">
    // Dependencies
    import { onMount } from 'svelte';
    import { Tabs, Accordion } from '@skeletonlabs/skeleton-svelte';
    import { fetch } from "@tauri-apps/plugin-http";
    import { marked } from 'marked';
    import { _ } from 'svelte-i18n';
    import { get } from 'svelte/store';

    let currentInfo = $state(0);

    // Information
    const changelogUrl = 'https://raw.githubusercontent.com/0auBSQ/OpenTaiko/main/CHANGELOG.md';
    let changelogContent = $state('');
    const hubChangelogUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Hub/main/CHANGELOG.md';
    let hubChangelogContent = $state('');
    const creditsUrl = 'https://raw.githubusercontent.com/OpenTaiko/OpenTaiko-Hub/main/CREDITS.md';
    let creditsContent = $state('');

    // External links open in the system browser and pick up the app's link styling
    const renderer = new marked.Renderer();
    const renderLink = renderer.link.bind(renderer);
    renderer.link = function (token) {
        const link = renderLink(token);
        return link.replace("<a", "<a target='_blank' class='text-blue-600 underline'");
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
            changelogContent = await marked(text);
        } else {
            changelogContent = `<p>${get(_)('info.error.changelog')}</p>`;
        }
        } catch (error) {
            changelogContent = `<p>${get(_)('info.error.generic', { values: { error: String(error) } })}</p>`;
        }
    }

    const updateHubChangeLogs = async () => {
        try {
            const response = await fetch(hubChangelogUrl);
        if (response.ok) {
            const text = await response.text();
            hubChangelogContent = await marked(text);
        } else {
            hubChangelogContent = `<p>${get(_)('info.error.changelog')}</p>`;
        }
        } catch (error) {
            hubChangelogContent = `<p>${get(_)('info.error.generic', { values: { error: String(error) } })}</p>`;
        }
    }

    const updateCredits = async () => {
        try {
            const response = await fetch(creditsUrl);
        if (response.ok) {
            const text = await response.text();
            creditsContent = await marked(text);
        } else {
            creditsContent = `<p>${get(_)('info.error.credits')}</p>`;
        }
        } catch (error) {
            creditsContent = `<p>${get(_)('info.error.generic', { values: { error: String(error) } })}</p>`;
        }
    }

    onMount(async () => {
        updateChangeLogs();
        updateHubChangeLogs();
        updateCredits();
    });

</script>

<Tabs value={String(currentInfo)} onValueChange={(details) => currentInfo = Number(details.value)} class="tab-bar w-full">
	<Tabs.List class="justify-center">
		<Tabs.Trigger value="0">
			<i class="fa-regular fa-file-lines"></i>
			<span>{$_('info.tab.changelog_game')}</span>
		</Tabs.Trigger>
		<Tabs.Trigger value="1">
			<i class="fa-regular fa-file-lines"></i>
			<span>{$_('info.tab.changelog_hub')}</span>
		</Tabs.Trigger>
		<Tabs.Trigger value="3">
			<i class="fa-regular fa-file-lines"></i>
			<span>{$_('info.tab.troubleshooting')}</span>
		</Tabs.Trigger>
		<Tabs.Trigger value="4">
			<i class="fa-regular fa-file-lines"></i>
			<span>{$_('info.tab.credits')}</span>
		</Tabs.Trigger>
	</Tabs.List>
</Tabs>

<!-- OpenTaiko Changelogs or OpenTaiko Hub Changelogs -->
{#if currentInfo === 0 || currentInfo === 1}
    <div class="content">
        <Accordion collapsible class="card rounded-container">
            <Accordion.Item value="legend">
                <h3>
                    <Accordion.ItemTrigger class="flex items-center justify-between gap-2">
                        <b>{$_('info.legend.title')}</b>
                        <Accordion.ItemIndicator class="group">
                            <i class="fa-solid fa-chevron-down transition group-data-[state=open]:rotate-180"></i>
                        </Accordion.ItemIndicator>
                    </Accordion.ItemTrigger>
                </h3>
                <Accordion.ItemContent>
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
                </Accordion.ItemContent>
            </Accordion.Item>
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
    /* Tailwind 4 compiles component styles in isolation: pull in the app theme for @apply */
    @reference "../../app.css";

    .content {@apply card w-full bg-surface-100-800 p-4;}
</style>
