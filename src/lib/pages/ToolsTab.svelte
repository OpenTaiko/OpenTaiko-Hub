<script>
    // Dependencies
    import { onMount } from 'svelte';
    import { TabGroup, Tab } from '@skeletonlabs/skeleton';
    import { getContext } from 'svelte';
    const { GetOS } = getContext('toast');

    // Tools
    import ToolCard from '$lib/components/ToolCard.svelte';
    import peepoScreenshotUrl from '$lib/peepo.png';
    import arrowScreenshotUrl from '$lib/arrow.png';
    import tjatoolsScreenshotUrl from '$lib/tjatools.png';

    let currentTool = 0;
	let optk_OS = "Win";

	onMount(async () => {
        optk_OS = await GetOS();
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
	<Tab bind:group={currentTool} name="tab1" value={0}>
		<svelte:fragment slot="lead"><i class="fa-regular fa-file-audio"></i></svelte:fragment>
		<span>Charting</span>
	</Tab>
	<Tab bind:group={currentTool} name="tab2" value={1}>
		<svelte:fragment slot="lead"><i class="fa-solid fa-envelope-open-text"></i></svelte:fragment>
		<span>Submit your content!</span>
	</Tab>
	<!-- ... -->
</TabGroup>
<!-- Charting -->
{#if currentTool === 0}
	<h2>Charting tutorial by ugyuu:</h2>
	<lite-youtube width="100%" videoid="U0i-z-tpxY8" playlabel="Play: Keynote (Google I/O '18)"></lite-youtube>
	{#if optk_OS === "Win"}
		<h2>Download PeepoDrumKit:</h2>
		<div class="w-full text-token grid grid-cols-1 md:grid-cols-2 gap-4">
			<ToolCard 
				Url="https://drive.google.com/uc?export=download&id=1TQuvKo1tBZrXZIMlUMJ3-1vU1jfsxI2H"
				ImageSrc={peepoScreenshotUrl}
				CardTitle="PeepoDrumKit"
				CardSubtitle="Official Latest"
				CardText ="The official latest version by samyuu, for regular charts without the experimental features."
				/>
			<ToolCard 
				Url="https://drive.google.com/uc?export=download&id=1P6ctv-qmxwH99z2mOSI5V3bPv5Rjfs11"
				ImageSrc={peepoScreenshotUrl}
				CardTitle="PeepoDrumKit"
				CardSubtitle="Unofficial Latest"
				CardText ="The unofficial latest version continued by Komi, adding experimental features supported by OpenTaiko."
				/>
		</div>
	{/if}
	<h2>Additional ressources:</h2>
	<div class="w-full text-token grid grid-cols-1 md:grid-cols-2 gap-4">
		<ToolCard 
			Url="https://whmhammer.github.io/tja-tools/"
			ImageSrc={tjatoolsScreenshotUrl}
			CardTitle="TJA Tools"
			CardSubtitle="WHMHammer's fork"
			CardText ="Visualize your charts and check stats about them on the go!"
			/>
		{#if optk_OS === "Win"}
		<ToolCard 
			Url="https://arrowvortex.ddrnl.com/"
			ImageSrc={arrowScreenshotUrl}
			CardTitle="Arrow Vortex"
			CardSubtitle="Official Latest"
			CardText ="The recommended tool to sync your charts trouble free!"
			/>
		{/if}
	</div>
{/if}
<!-- Submit your content -->
{#if currentTool === 1}
	<div class="w-full text-token grid grid-cols-1 md:grid-cols-2 gap-4">
		<ToolCard 
			Url="https://forms.gle/WXNUwjJyLdJoeRSM6"
			ImageSrc={peepoScreenshotUrl}
			CardTitle="OpenTaiko soundtrack"
			CardSubtitle="Song submission form"
			CardText ="You can submit your songs here for the OpenTaiko soundtrack! Charts are usually made within 2 weeks if the song is accepted."
			/>
		<ToolCard 
			Url="https://forms.gle/1HbDtS7FtJDNfhUb7"
			ImageSrc={peepoScreenshotUrl}
			CardTitle="OpenTaiko soundtrack"
			CardSubtitle="Song suggestion form"
			CardText ="You can recommand artists here whose songs you think would fit for OpenTaiko!"
			/>
	</div>
{/if}

<style>

</style>
