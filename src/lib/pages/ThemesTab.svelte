<script lang="ts">
    import { onMount } from 'svelte';
    import { LoadSettings, SaveSettings } from '$lib/settings';
    import { applyTheme, applyMode, getAppliedTheme, normalizeThemeName, DEFAULT_THEME, DEFAULT_MODE, type ThemeMode } from "$lib/utils/appearance";

    import { SegmentedControl } from '@skeletonlabs/skeleton-svelte';

    import { _ , locale } from 'svelte-i18n';

    interface Props {
        /** Selected navigation tile; the tab renders only while it is the Themes one. */
        currentTile?: number;
    }

    let { currentTile = 0 }: Props = $props();

    let currentTheme = $state<string>(DEFAULT_THEME);

    const TryFetchingCurrentTheme = async () => {
        // Preferences written by older Hub versions use the Skeleton v2 preset names;
        // none saved yet means the default
        const { theme } = await LoadSettings();
        currentTheme = theme ? normalizeThemeName(theme) : DEFAULT_THEME;
        applyTheme(currentTheme);
    }

    const ThemeChanger = async (e: Event) => {
        const themevalue = (e.currentTarget as HTMLSelectElement).value;
        // Picking the theme already in use is not a mistake worth a warning
        if (getAppliedTheme() == themevalue) return;

        await SaveSettings({ theme: themevalue });

        currentTheme = themevalue;
        applyTheme(themevalue);
    }

    // Theme mode
    let currentThemeMode = $state<ThemeMode | 'Loading...'>('Loading...');

    const TryFetchingCurrentThemeMode = async () => {
        const { mode } = await LoadSettings();
        currentThemeMode = mode === 'light' ? 'light' : DEFAULT_MODE;
        applyMode(currentThemeMode);
    }

    const ThemeModeChanger = async (mode: string) => {
        currentThemeMode = mode === 'light' ? 'light' : 'dark';
        await SaveSettings({ mode });

        applyMode(mode);
    }

    // Language persistence
    const SaveLocale = async (selectedLocale: string | null | undefined) => {
        await SaveSettings({ locale: selectedLocale ?? undefined });
    }

    onMount(async () => {
        TryFetchingCurrentTheme();
        TryFetchingCurrentThemeMode();
    });
</script>

{#if currentTile === 7}
        <div id="themetab">
            <section class="card w-full">
                <div class="p-4 space-y-4">
                    <h1>{$_('nav.themes')}</h1>
                    <h2>{$_('themes.title')}</h2>
                    <div class="flex gap-2">
                        <select size="11" class="select w-full max-w-[265px]" aria-label={$_('themes.title')} value={currentTheme} onchange={ThemeChanger}>
                            <optgroup label={$_('themes.optgroup.optk')}>
                                <option value="gleamingsky">Gleaming Sky</option>
                                <option value="dashy">888</option>
                                <option value="deceiver">Deceiver</option>
                                <option value="onyx">Onyx</option>
                                <option value="pearl">Pearl</option>
                                <option value="optkkun">OpenTaiko-Kun</option>
                            </optgroup>

                            <optgroup label={$_('themes.optgroup.skeleton')}>
                                <option value="legacy">Legacy</option>
                                <option value="wintry">Wintry</option>
                                <option value="modern">Modern</option>
                                <option value="rocket">Rocket</option>
                                <option value="seafoam">Seafoam</option>
                                <option value="vintage">Vintage</option>
                                <option value="sahara">Sahara</option>
                                <option value="hamlindigo">Hamlindigo</option>
                                <option value="nouveau">Gold Nouveau</option>
                                <option value="crimson">Crimson</option>
                            </optgroup>
                        </select>

                        <div class="card w-full p-4 rounded-container">
                            <h1>{$_('themes.preview.title')}</h1>

                            <hr class="m-4">

                            <h2>{$_('themes.preview.color')}</h2>
                            <div class="grid grid-cols-1 grid-rows-3 gap-2">
                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>{$_('themes.preview.accents')}</span>
                                        <span class="badge p-2 preset-filled-primary-500">{$_('themes.preview.accent_primary')}</span>
                                        <span class="badge p-2 preset-filled-secondary-500">{$_('themes.preview.accent_secondary')}</span>
                                        <span class="badge p-2 preset-filled-tertiary-500">{$_('themes.preview.accent_tertiary')}</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>{$_('themes.preview.buttons')}</span>
                                        <span class="badge p-2 button-blue">{$_('themes.preview.btn_action')}</span>
                                        <span class="badge p-2 button-green">{$_('themes.preview.btn_download')}</span>
                                        <span class="badge p-2 button-gray">{$_('themes.preview.btn_repeat')}</span>
                                        <span class="badge p-2 button-red">{$_('themes.preview.btn_error')}</span>
                                    </p>
                                </div>

                                <div class="flex">
                                    <p class="flex gap-1 badge card p-2">
                                        <span>{$_('themes.preview.notifs')}</span>
                                        <span class="badge p-2 preset-filled-success-500">{$_('themes.preview.notif_success')}</span>
                                        <span class="badge p-2 preset-filled-warning-500">{$_('themes.preview.notif_warning')}</span>
                                        <span class="badge p-2 preset-filled-error-500">{$_('themes.preview.notif_error')}</span>
                                    </p>
                                </div>
                            </div>

                            <hr class="m-4">

                            <h2>{$_('themes.credits.title')}</h2>
                            <p>{$_('themes.credits.text')}</p>
                            <a href="https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md" target="_blank">https://github.com/OpenTaiko/OpenTaiko-Hub/blob/main/README.md</a>
                        </div>
                    </div>

                    <div>
                        <div class="flex items-center gap-2">
                            <SegmentedControl
                                value={currentThemeMode}
                                onValueChange={(details) => ThemeModeChanger(details.value ?? DEFAULT_MODE)}
                            >
                                <SegmentedControl.Control>
                                    <SegmentedControl.Indicator />
                                    <SegmentedControl.Item value="dark" title={$_('themes.mode.dark')} aria-label={$_('themes.mode.dark')}>
                                        <SegmentedControl.ItemText><i class="fa-solid fa-moon"></i></SegmentedControl.ItemText>
                                        <SegmentedControl.ItemHiddenInput />
                                    </SegmentedControl.Item>
                                    <SegmentedControl.Item value="light" title={$_('themes.mode.light')} aria-label={$_('themes.mode.light')}>
                                        <SegmentedControl.ItemText><i class="fa-solid fa-sun"></i></SegmentedControl.ItemText>
                                        <SegmentedControl.ItemHiddenInput />
                                    </SegmentedControl.Item>
                                </SegmentedControl.Control>
                            </SegmentedControl>

                            {#if currentThemeMode === "dark"}
                                <p class="flex items-center px-2">{$_('themes.mode.current', { values: { mode: $_('themes.mode.dark') } })}</p>
                            {:else if currentThemeMode === "light"}
                                <p class="flex items-center px-2">{$_('themes.mode.current', { values: { mode: $_('themes.mode.light') } })}</p>
                            {/if}
                        </div>
                    </div>

                    <!-- Its own section: it used to sit inside the theme preview, as if it
                         were part of the theme -->
                    <hr>
                    <h2>{$_('lang.label')}</h2>
                    <select class="select w-full max-w-[200px]" aria-label={$_('lang.label')} bind:value={$locale} onchange={() => SaveLocale($locale)}>
                        <option value="en">{$_('lang.en')}</option>
                        <option value="ja">{$_('lang.ja')}</option>
                        <option value="zh-Hans">{$_('lang.zh_Hans')}</option>
                        <option value="zh-Hant">{$_('lang.zh_Hant')}</option>
                        <option value="fr">{$_('lang.fr')}</option>
                        <option value="es">{$_('lang.es')}</option>
                        <option value="de">{$_('lang.de')}</option>
                        <option value="nl">{$_('lang.nl')}</option>
                        <option value="ko">{$_('lang.ko')}</option>
                        <option value="ru">{$_('lang.ru')}</option>
                    </select>
                </div>
            </section>
        </div>
{:else}
    <!-- Hide content -->
{/if}

<style>
    /* Theme page CSS */
    option {text-align: center;}
</style>
