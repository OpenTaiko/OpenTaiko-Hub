<script lang="ts">
    import { onMount } from 'svelte';
    import { readTextFile, writeTextFile, mkdir } from '@tauri-apps/plugin-fs';

    import { getContext } from 'svelte';
    import type { ToastContext } from '$lib/types';
    const { TriggerWarning, TriggerSuccess } = getContext<ToastContext>('toast');

    import { GetPreferencesPath } from "$lib/utils/path";
    import { applyTheme, applyMode, getAppliedTheme, normalizeThemeName, DEFAULT_THEME, DEFAULT_MODE, type ThemeMode } from "$lib/utils/appearance";

    import { path } from '@tauri-apps/api';

    import { SegmentedControl } from '@skeletonlabs/skeleton-svelte';

    import { _ , locale } from 'svelte-i18n';
    import { get } from 'svelte/store';

    // Debug mode code
    let debugMode = $state(false);

    const DisableDebugMode = async () => {
        TriggerSuccess(get(_)('themes.success.debug_disabled'));
        debugMode = false;
    }

    let currentThemeModeDebug = $state<ThemeMode>('dark');

    const ThemeChangerDebug = async (e: Event) => {
        const themevalue = (e.currentTarget as HTMLSelectElement).value;

        if (getAppliedTheme() == themevalue) {
            TriggerWarning(get(_)('themes.warn.same_theme_debug'));
        }
        else {
            currentTheme = themevalue;
            applyTheme(themevalue);
            console.log("DEBUG: theme has been changed")
        }
    }

    const ThemeModeChangerDebug = async (mode: string) => {
        currentThemeModeDebug = mode === 'light' ? 'light' : 'dark';
        applyMode(mode);
        console.log(`DEBUG: mode has been changed to ${mode}`)
    }

    interface Props {
        /** Selected navigation tile; the tab renders only while it is the Themes one. */
        currentTile?: number;
    }

    let { currentTile = 0 }: Props = $props();

    let currentTheme = $state<string>(DEFAULT_THEME);

    const TryFetchingCurrentTheme = async () => {
        const prefsDir = await GetPreferencesPath();
        await mkdir(prefsDir, { recursive: true });
        const themeFilePath = await path.join(prefsDir, 'theme.json');
        try {
            const fileContentTheme = await readTextFile(themeFilePath);
            const jsonDataTheme = JSON.parse(fileContentTheme) as { theme?: string };
            // Preferences written by older Hub versions use the Skeleton v2 preset names
            currentTheme = normalizeThemeName(jsonDataTheme.theme);
        } catch {
            // File missing on first run — silently use default and write it
            currentTheme = DEFAULT_THEME;
            try { await writeTextFile(themeFilePath, JSON.stringify({ theme: currentTheme })); } catch {}
        }
        applyTheme(currentTheme);
    }

    const ThemeChanger = async (e: Event) => {
        const themevalue = (e.currentTarget as HTMLSelectElement).value;

        if (getAppliedTheme() == themevalue) {
            TriggerWarning(get(_)('themes.warn.same_theme'));
        }
        else {
            const prefsDir = await GetPreferencesPath();
            await mkdir(prefsDir, { recursive: true });
            const theme_settings = await path.join(prefsDir, 'theme.json');
            await writeTextFile(theme_settings, JSON.stringify({ theme: themevalue }));

            currentTheme = themevalue;
            applyTheme(themevalue);
            console.log("theme has been changed")
        }
    }

    // Theme mode
    let currentThemeMode = $state<ThemeMode | 'Loading...'>('Loading...');

    const TryFetchingCurrentThemeMode = async () => {
        const prefsDir = await GetPreferencesPath();
        await mkdir(prefsDir, { recursive: true });
        const modeFilePath = await path.join(prefsDir, 'thememode.json');
        try {
            const fileContentMode = await readTextFile(modeFilePath);
            const jsonDataMode = JSON.parse(fileContentMode) as { thememode?: string };
            currentThemeMode = jsonDataMode.thememode === 'light' ? 'light' : DEFAULT_MODE;
        } catch {
            // File missing on first run — silently use default and write it
            currentThemeMode = DEFAULT_MODE;
            try { await writeTextFile(modeFilePath, JSON.stringify({ thememode: currentThemeMode })); } catch {}
        }

        applyMode(currentThemeMode);
        console.log(`mode has been changed to ${currentThemeMode}`)
    }

    const ThemeModeChanger = async (mode: string) => {
        currentThemeMode = mode === 'light' ? 'light' : 'dark';
        const prefsDir = await GetPreferencesPath();
        await mkdir(prefsDir, { recursive: true });
        const thememode_settings = await path.join(prefsDir, 'thememode.json');
        await writeTextFile(thememode_settings, JSON.stringify({ thememode: mode }));

        applyMode(mode);
        console.log(`mode has been changed to ${mode}`)
    }

    // Language persistence
    const SaveLocale = async (selectedLocale: string | null | undefined) => {
        try {
            const prefsDir = await GetPreferencesPath();
            await mkdir(prefsDir, { recursive: true });
            const langFilePath = await path.join(prefsDir, 'language.json');
            await writeTextFile(langFilePath, JSON.stringify({ locale: selectedLocale }));
        } catch (err) {
            console.error('Failed to save locale:', err);
        }
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
                    <h1>{debugMode ? $_('themes.title_debug') : $_('themes.title')}</h1>
                    <div class="flex gap-2">
                        <select size="11" class="select w-full max-w-[265px]" value={currentTheme} onchange={debugMode ? ThemeChangerDebug : ThemeChanger} onclick={debugMode ? () => {} : TryFetchingCurrentTheme}>
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

                            <hr class="m-4">

                            <h2>{$_('lang.label')}</h2>
                            <select class="select w-full max-w-[200px]" bind:value={$locale} onchange={() => SaveLocale($locale)}>
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
                    </div>

                    <div class={debugMode ? "flex gap-2" : ""}>
                        <div class="flex items-center gap-2">
                            <SegmentedControl
                                value={debugMode ? currentThemeModeDebug : currentThemeMode}
                                onValueChange={(details) => (debugMode ? ThemeModeChangerDebug : ThemeModeChanger)(details.value ?? DEFAULT_MODE)}
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

                            {#if (debugMode ? currentThemeModeDebug : currentThemeMode) === "dark"}
                                <p class="flex items-center px-2">{$_('themes.mode.current', { values: { mode: $_('themes.mode.dark') } })}</p>
                            {:else if (debugMode ? currentThemeModeDebug : currentThemeMode) === "light"}
                                <p class="flex items-center px-2">{$_('themes.mode.current', { values: { mode: $_('themes.mode.light') } })}</p>
                            {/if}
                        </div>

                        {#if debugMode}
                            <button type="button" onclick={DisableDebugMode} class="button-red button-main"><i class="fa-solid fa-code"></i> {$_('themes.debug.disable_btn')}</button>
                        {/if}
                    </div>
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
