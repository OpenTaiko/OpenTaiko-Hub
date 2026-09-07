<script>
    // Dependencies
    import { onMount } from 'svelte';
    import { Tabs } from '@skeletonlabs/skeleton-svelte';
    import { GetOS } from '$lib/utils/path.js';
    import { _ } from 'svelte-i18n';

    // Tools
    import ToolCard from '$lib/components/ToolCard.svelte';
    import peepoSamyuuScreenshotUrl from '$lib/peepoSamyuu.png';
    import peepoScreenshotUrl from '$lib/peepo.png';
    import peepoPreviewScreenshotUrl from '$lib/peepoPreview.png';
    import peepoSDL3ScreenshotUrl from '$lib/peepoSDL3.png';
    import arrowScreenshotUrl from '$lib/arrow.png';
    import tjatoolsScreenshotUrl from '$lib/tjatools.png';
	import subeditScreenshotUrl from '$lib/subedit.png';
	import songSubmissionScreenshotUrl from '$lib/songsubmission.png';
	import songSuggestionScreenshotUrl from '$lib/songsuggestion.png';

    let currentTab = $state(0);
	let optk_OS = $state("Win");

	onMount(async () => {
        optk_OS = await GetOS();
    });

</script>

<Tabs value={String(currentTab)} onValueChange={(details) => currentTab = Number(details.value)} class="tab-bar w-full">
	<Tabs.List class="justify-center">
		<Tabs.Trigger value="0">
			<i class="fa-regular fa-file-audio"></i>
			<span>{$_('tools.tab.charting')}</span>
		</Tabs.Trigger>
		<Tabs.Trigger value="1">
			<i class="fa-solid fa-music"></i>
			<span>{$_('tools.tab.lyrics')}</span>
		</Tabs.Trigger>
		<Tabs.Trigger value="2">
			<i class="fa-solid fa-envelope-open-text"></i>
			<span>{$_('tools.tab.submit')}</span>
		</Tabs.Trigger>
	</Tabs.List>
</Tabs>
<!-- Charting -->
{#if currentTab === 0}
	<div class="content">
		<h1 class="mb-3">{$_('tools.charting.primary')}</h1>
		<div class="w-full grid grid-cols-2 md:grid-cols-2 gap-4">
			<div class="card bg-initial card-hover overflow-hidden">
				<header>
					<lite-youtube width="100%" videoid="U0i-z-tpxY8" playlabel="Play: Keynote (Google I/O '18)"></lite-youtube>
				</header>
				<div class="p-4 space-y-4 text-black dark:text-white">
					<h2 data-toc-ignore="">{$_('tools.charting.tutorial.title')}</h2>
					<h6 data-toc-ignore="">{$_('tools.charting.tutorial.subtitle')}</h6>
				</div>
			</div>
		</div>

		<h2 class="my-3">{$_('tools.charting.peepo_dl')}</h2>
		<div class="w-full grid grid-cols-2 md:grid-cols-2 gap-4">
			{#if optk_OS === "Win"}	
				<ToolCard 
					Url="https://drive.google.com/uc?export=download&id=1TQuvKo1tBZrXZIMlUMJ3-1vU1jfsxI2H"
					ImageSrc={peepoSamyuuScreenshotUrl}
					CardSubtitle={$_('tools.card.peepo_origin.subtitle')}
					CardText={$_('tools.card.peepo_origin.text')}
				/>
				<ToolCard
					Url="https://github.com/0auBSQ/PeepoDrumKit/releases/latest"
					ImageSrc={peepoScreenshotUrl}
					CardSubtitle={$_('tools.card.peepo_optk.subtitle')}
					CardText={$_('tools.card.peepo_optk.text')}
				/>
				<ToolCard
					Url="https://github.com/IepIweidieng/PeepoDrumKit/releases/latest"
					ImageSrc={peepoPreviewScreenshotUrl}
					CardSubtitle={$_('tools.card.peepo_preview.subtitle')}
					CardText={$_('tools.card.peepo_preview.text')}
				/>
			{/if}
			<ToolCard
				Url="https://github.com/IepIweidieng/PeepoDrumKit-sdl3/releases/latest"
				ImageSrc={peepoSDL3ScreenshotUrl}
				CardSubtitle={$_('tools.card.peepo_sdl3.subtitle')}
				CardText={$_('tools.card.peepo_sdl3.text')}
			/>
		</div>

		<h1 class="my-3">{$_('tools.charting.additional')}</h1>
		<div class="w-full grid grid-cols-1 md:grid-cols-2 gap-4">
			<ToolCard
				Url="https://whmhammer.github.io/tja-tools/"
				ImageSrc={tjatoolsScreenshotUrl}
				CardTitle={$_('tools.card.tja_tools.title')}
				CardSubtitle={$_('tools.card.tja_tools.subtitle')}
				CardText={$_('tools.card.tja_tools.text')}
			/>
			{#if optk_OS === "Win"}
				<ToolCard
					Url="https://arrowvortex.ddrnl.com/"
					ImageSrc={arrowScreenshotUrl}
					CardTitle={$_('tools.card.arrow_vortex.title')}
					CardSubtitle={$_('tools.card.arrow_vortex.subtitle')}
					CardText={$_('tools.card.arrow_vortex.text')}
				/>
			{/if}
		</div>
	</div>
{/if}
{#if currentTab === 1}
	<div class="content">
		<h1 class="my-3">{$_('tools.lyrics.dl')}</h1>
		<div class="w-full grid grid-cols-1 md:grid-cols-2 gap-4">
			<ToolCard
				Url="https://github.com/SubtitleEdit/subtitleedit"
				ImageSrc={subeditScreenshotUrl}
				CardTitle={$_('tools.card.subtitle_edit.title')}
				CardSubtitle={$_('tools.card.subtitle_edit.subtitle')}
				CardText={$_('tools.card.subtitle_edit.text')}
			/>
		</div>
	</div>
{/if}
<!-- Submit your content -->
{#if currentTab === 2}
	<div class="content">
		<h1 class="my-3">{$_('tools.submit.soundtrack')}</h1>
		<div class="w-full grid grid-cols-1 md:grid-cols-2 gap-4">
			<ToolCard
				Url="https://forms.gle/WXNUwjJyLdJoeRSM6"
				ImageSrc={songSubmissionScreenshotUrl}
				CardTitle={$_('tools.card.submission.title')}
				CardSubtitle={$_('tools.card.submission.subtitle')}
				CardText={$_('tools.card.submission.text')}
			/>
			<ToolCard
				Url="https://forms.gle/1HbDtS7FtJDNfhUb7"
				ImageSrc={songSuggestionScreenshotUrl}
				CardTitle={$_('tools.card.suggestion.title')}
				CardSubtitle={$_('tools.card.suggestion.subtitle')}
				CardText={$_('tools.card.suggestion.text')}
			/>
		</div>
	</div>
{/if}

<style>
    /* Tailwind 4 compiles component styles in isolation: pull in the app theme for @apply */
    @reference "../../app.css";

	.content {@apply card w-full bg-surface-100-800 p-4;}
</style>
