<script>
    import { onMount } from 'svelte';
    import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';

    import { getContext } from 'svelte';
    const { TriggerError, TriggerWarning, TriggerSuccess } = getContext('toast');

    import { GetRootPath } from "$lib/utils/path.js";

    import { path } from '@tauri-apps/api';

    // Themes
    import { RadioGroup, RadioItem } from '@skeletonlabs/skeleton';
    import { setModeCurrent } from '@skeletonlabs/skeleton';

    let currentTheme = '';
    let themeDetails = 'Loading...';
    let themeSettingsNotFound = false;
    let themeModeSettingsNotFound = false;

    const TryFetchingCurrentTheme = async () => {
        try {
            themeSettingsNotFound = false;
            const filePathTheme = './settings/theme.json';
            const res = await GetRootPath();
            const themeFilePath = await path.join(res,filePathTheme)
            const fileContentTheme = await readTextFile(themeFilePath);
            const jsonDataTheme = JSON.parse(fileContentTheme);
            currentTheme = jsonDataTheme.theme;
            themeDetails = `${currentTheme}`;
        } catch (err) {
            themeSettingsNotFound = true;
            currentTheme = 'hubdefault';
            themeDetails = "theme.json doesn't exist";
            TriggerError("The theme.json file (or the settings folder) doesn't exist or is corrupted!<br>Please reinstall the OpenTaiko Hub to fix this.");
        }

        document.body.dataset.theme = currentTheme;
    }

    const ThemeChanger = async () => {
        const themeselect = document.getElementById("themeselect");
        const themevalue = themeselect.value;
        const themetarget = document.getElementById('themetarget');
        const themedata = themetarget.getAttribute('data-theme');
        
        if (themedata == themevalue) {
            TriggerWarning("Selected theme is the same as current theme.");
        } 
        else {
            const theme_settings = ("./settings/theme.json");
            const json_theme = {"theme":themevalue}
            await writeTextFile(theme_settings, JSON.stringify(json_theme));
            
            document.body.dataset.theme = themevalue;
            console.log("theme has been changed")
        }
    }

    // Theme mode
    const thememode_settings = ("./settings/thememode.json");
    const json_modelight = {"thememode":"light"};
    const json_modedark = {"thememode":"dark"};
    
    let currentThemeMode = 'Loading...';
    let themeModeDetails = 'Loading...';

    const TryFetchingCurrentThemeMode = async () => {
        try {
            themeModeSettingsNotFound = false;
            const filePathMode = './settings/thememode.json';
            const res = await GetRootPath();
            const modeFilePath = await path.join(res,filePathMode)
            const fileContentMode = await readTextFile(modeFilePath);
            const jsonDataMode = JSON.parse(fileContentMode);
            currentThemeMode = jsonDataMode.thememode;
            themeModeDetails = `${currentThemeMode}`;
        } catch (err) {
            themeModeSettingsNotFound = true;
            currentThemeMode = 'dark';
            themeModeDetails = "thememode.json doesn't exist";
            TriggerError("The thememode.json file (or the settings folder) doesn't exist or is corrupted!<br>Please reinstall the OpenTaiko Hub to fix this.");
        }

        if (currentThemeMode === "dark") {
            setModeCurrent(false);
            console.log("mode has been changed to dark")
        }
        else if (currentThemeMode === "light") {
            setModeCurrent(true);
            console.log("mode has been changed to light")
        }
    }

    const ThemeModeChanger = async () => {
        if (currentThemeMode === "dark") {
            await writeTextFile(thememode_settings, JSON.stringify(json_modedark));

            setModeCurrent(false);
            console.log("mode has been changed to dark")
        }
        else if (currentThemeMode === "light") {
            await writeTextFile(thememode_settings, JSON.stringify(json_modelight));
            
            setModeCurrent(true);
            console.log("mode has been changed to light")
        }
    }

    onMount(async () => {
        TryFetchingCurrentTheme();
        TryFetchingCurrentThemeMode();
    });
</script>

<div class="grid h-screen grid-rows-[auto_1fr_auto]">
    <section class="card w-full">
        <div class="p-4 space-y-4">
            <h1>Select a theme.</h1>
            <div class="flex gap-2">
                <!-- value={currentTheme} on:change={themeChanger} on:click={TryFetchingCurrentTheme} -->
                <select id="themeselect" size="12" class="select w-full max-w-[265px]" value={currentTheme} on:change={ThemeChanger} on:click={TryFetchingCurrentTheme}>
                    <optgroup label="OpenTaiko Hub themes:">
                        <option value="hubdefault">Default</option>
                        <option value="gleamingsky">Gleaming Sky</option>
                        <option value="dashy">888</option>
                        <option value="deceiver">Deceiver</option>
                        <option value="onyx">Onyx</option>
                        <option value="pearl">Pearl</option>
                    </optgroup>

                    <optgroup label="Skeleton preset themes:">
                        <option value="skeleton">Legacy</option>
                        <option value="wintry">Wintry</option>
                        <option value="modern">Modern</option>
                        <option value="rocket">Rocket</option>
                        <option value="seafoam">Seafoam</option>
                        <option value="vintage">Vintage</option>
                        <option value="sahara">Sahara</option>
                        <option value="hamlindigo">Hamlindigo</option>
                        <option value="gold-nouveau">Gold Nouveau</option>
                        <option value="crimson">Crimson</option>
                    </optgroup>
                </select>

                <div class="card w-full p-4 border-2 border-surface-300 dark:border-surface-600">
                    <h1>Welcome to the OpenTaiko Hub Themes tab!</h1>
                    <p>This tab has a decent amount of content in it, so here's a guide to everything here.</p>
                            
                    <hr class="m-4">
                            
                    <h1>Menu guide</h1>
                    <p><i class="fa-solid fa-arrow-left"></i> <b>On this side there's the theme selector, and light/dark mode selector.</b></p>
                    <p class="alignright"><b>And on this side there's the color preview.</b> <i class="fa-solid fa-arrow-right"></i></p>
                            
                    <hr class="m-4">

                    <h1>Theme credits</h1>
                    <p>The "OpenTaiko Hub themes" are themes made for the OpenTaiko Hub. 
                    <br>Credits for them can be found in the "Information" tab under "Credits".</p>
                    <br><p>The Skeleton preset themes are themes provided by <a href="https://www.skeleton.dev/" target='_blank' class='text-blue-600'>Skeleton UI.</a> <span class="smalltext"><i>(Specifically Skeleton v2.)</i></span>
                    <br>You can find info about the provided Skeleton themes <a href="https://www.skeleton.dev/docs/design/themes" target='_blank' class='text-blue-600'>here.</a></p>
                </div>

                <div class="grid grid-cols-1 grid-rows-7 gap-4 w-[120px]">
                    <span class="badge p-2 border-2 border-surface-300 dark:border-surface-600">Surface</span>
                    <span class="badge p-2 variant-filled-primary">Primary</span>
                    <span class="badge p-2 variant-filled-secondary">Secondary</span>
                    <span class="badge p-2 variant-filled-tertiary">Tertiary</span>
                    <span class="badge p-2 variant-filled-success">Success</span>
                    <span class="badge p-2 variant-filled-warning">Warning</span>
                    <span class="badge p-2 variant-filled-error">Error</span>
                </div>
            </div>

                    
            <RadioGroup>
                <RadioItem bind:group={currentThemeMode} on:change={ThemeModeChanger} name="justify" value={"dark"}><i class="fa-solid fa-moon"></i></RadioItem>
                <RadioItem bind:group={currentThemeMode} on:change={ThemeModeChanger} name="justify" value={"light"}><i class="fa-solid fa-sun"></i></RadioItem>
                   
                {#if currentThemeMode === "dark"}
                    <p class="flex items-center px-2">Current Mode: Dark</p>
                {:else if currentThemeMode === "light"}
                    <p class="flex items-center px-2">Current Mode: Light</p>
                {:else}
                    <p class="flex items-center px-2">Current Mode: Loading...</p>
                {/if}
            </RadioGroup>
        </div>
    </section>
</div>

<style>
    /* Main CSS */
    .alignright {text-align: right;}
    
    /* Theme page CSS */
    option {text-align: center;}
</style>