<script lang="ts">
	import { tick } from 'svelte';
	import { _, locale } from 'svelte-i18n';
	import type { SoundtrackEntry } from '$lib/types';
	import { localizedTitle, localizedSubtitle } from '$lib/utils/titles';

	interface Props {
		/** The catalog entry, or null for a song of the user's own that the soundtrack
		 *  does not have: it plays from disk and shows Title and Subtitle instead. */
		songInfo: SoundtrackEntry | null;
		/** Local audio for an installed chart. Falls back to the soundtrack repo only
		 *  when the chart is not on disk, so a downloaded library plays offline and
		 *  does not hammer the repository. */
		Src?: string | null;
		/** Reads the local audio when it is first played, for an installed chart: read on
		 *  selection, browsing the library read every song's whole audio file. Src then
		 *  carries what it read. */
		LoadSrc?: () => Promise<string | null>;
		Title?: string;
		Subtitle?: string | null;
	}

	let { songInfo, Src = null, LoadSrc, Title = '', Subtitle = null }: Props = $props();

	// A chart whose audio is on disk never streams it from the repository meanwhile
	let source = $derived(
		Src
		?? (LoadSrc ? null : songInfo ? `https://github.com/OpenTaiko/OpenTaiko-Soundtrack/raw/refs/heads/main/${songInfo.chartAudioFilePath}` : null)
	);

	const TogglePlay = async () => {
		if (paused && !source && LoadSrc) {
			if (!(await LoadSrc())) return;
			// Src arrives as a prop: let it reach the <audio> before playing
			await tick();
		}
		paused = !paused;
	};

	let time = $state(0);
	let duration = $state(0);
	let paused = $state(true);

	// Changing the src on a live <audio> keeps `paused` false, so picking another song
	// while one played left the button showing Pause for a track that was not playing.
	let loadedId = $state<string | null>(null);
	$effect(() => {
		if (source === loadedId) return;
		loadedId = source;
		paused = true;
		time = 0;
	});

	let title = $derived(songInfo ? localizedTitle(songInfo, $locale) : Title);
	let subtitle = $derived(songInfo ? localizedSubtitle(songInfo, $locale) : (Subtitle ?? ''));

	function format(time: number): string {
		if (isNaN(time)) return '...';

		const minutes = Math.floor(time / 60);
		const seconds = Math.floor(time % 60);

		return `${minutes}:${seconds < 10 ? `0${seconds}` : seconds}`;
	}
</script>

<div class="player" class:paused>
	<audio
		src={source}
		bind:currentTime={time}
		bind:duration
		bind:paused
		preload="none"
		onended={() => {
			time = 0;
		}}
	></audio>
	
	<button
		class="play"
		aria-label={paused ? $_('player.play') : $_('player.pause')}
		onclick={TogglePlay}
	></button>

	<div class="info">
		<div class="description">
			<strong>{title}</strong>
			<br/><small>{subtitle}</small>
		</div>

		<div class="time">
			<span>{format(time)}</span>
			<div
				class="slider"
				role="slider"
				tabindex="0"
				aria-label={$_('player.position')}
				aria-valuemin={0}
				aria-valuemax={duration || 0}
				aria-valuenow={time}
				aria-valuetext={format(time)}
				onkeydown={(e) => {
					if (!duration) return;
					const step = { ArrowLeft: -5, ArrowDown: -5, ArrowRight: 5, ArrowUp: 5 }[e.key];
					if (step !== undefined) time = Math.min(duration, Math.max(0, time + step));
					else if (e.key === 'Home') time = 0;
					else if (e.key === 'End') time = duration;
					else return;
					e.preventDefault();
				}}
				onpointerdown={e => {
					const div = e.currentTarget;
					
					function seek(e: PointerEvent) {
						const { left, width } = div.getBoundingClientRect();

						let p = (e.clientX - left) / width;
						if (p < 0) p = 0;
						if (p > 1) p = 1;
						
						time = p * duration;
					}

					seek(e);

					window.addEventListener('pointermove', seek);

					window.addEventListener('pointerup', () => {
						window.removeEventListener('pointermove', seek);
					}, {
						once: true
					});
				}}
			>
				<div class="progress" style="--progress: {time / duration}%"></div>
			</div>
			<span>{duration ? format(duration) : '--:--'}</span>
		</div>
	</div>
</div>

<style>
	.player {
		display: grid;
		grid-template-columns: 2.5em 1fr;
		align-items: center;
		gap: 1em;
		padding: 0.5em 1em 0.5em 0.5em;
		border-radius: 2em;
		background: var(--bg-1);
		transition: filter 0.2s;
		color: var(--fg-3);
		user-select: none;
	}

	.player:not(.paused) {
		color: var(--fg-1);
		filter: drop-shadow(0.5em 0.5em 1em rgba(0,0,0,0.1));
	}
	
	button.play {
		width: 100%;
		aspect-ratio: 1;
		background-repeat: no-repeat;
		background-position: 50% 50%;
        background-color: rgb(115, 193, 252);
		border-radius: 50%;
	}

    button.play:hover {
        background-color: rgb(176, 221, 255);
    }

    button.play:active {
        background-color: rgb(59, 165, 247);
    }
	
	.player:not(.paused) .play {
		background-image: url(./pause.svg);
	}

	.player.paused .play {
		background-image: url(./play.svg);
	}

	.info {
		overflow: hidden;
	}

	.description {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		line-height: 1.2;
	}

	.time {
		display: flex;
		align-items: center;
		gap: 0.5em;
	}

	.time span {
		font-size: 0.7em;
	}

	.slider {
		flex: 1;
		height: 0.5em;
		background: rgb(26, 28, 61);
		border-radius: 0.5em;
		overflow: hidden;
	}

	.progress {
		width: calc(100 * var(--progress));
		height: 100%;
		background: rgb(62, 112, 187);
	}
</style>