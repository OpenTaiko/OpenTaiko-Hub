<script>
    import "../app.css";
    import "../lite-yt-embed.css";
    import "../lite-yt-embed.js";
    
    import { onMount, setContext } from 'svelte';
    import { autoModeWatcher, getToastStore } from '@skeletonlabs/skeleton';
    import { initializeStores, Toast } from '@skeletonlabs/skeleton';
    import { download } from "@tauri-apps/plugin-upload";
    import { type } from '@tauri-apps/plugin-os';

    initializeStores();

    const toastStore = getToastStore();

    const TriggerError = (msg) => {
        console.error(msg);
        const t = {
            message: msg,
            timeout: 4000,
            background: 'variant-filled-error',
        };
        toastStore.trigger(t);
    };

    const TriggerWarning = (msg) => {
        console.warn(msg);
        const t = {
            message: msg,
            timeout: 4000,
            background: 'variant-filled-warning',
        };
        toastStore.trigger(t);
    };

    const TriggerSuccess = (msg) => {
        console.log(msg);
        const t = {
            message: msg,
            timeout: 4000,
            background: 'variant-filled-success',
        };
        toastStore.trigger(t);
    };

    const wait = (ms) => {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    const backoffDownload = async (url, path, prfunc, n = 10, t = 1000) => {
        for (let attempt = 1; attempt <= n; attempt++) {
            try {
                await download(url, path, prfunc);
                return true;
            } 
            catch (error) {
                if (attempt === n) {
                    TriggerError(`Download failed, aborting`);
                    return false; 
                }
                TriggerWarning(`Download failed, retrying... (${attempt}/${n})`);

                await wait(t);
            }
        }
    }

    const GetOS = async () => {
        const currentPlatform = await type();
        switch (currentPlatform) {
            case "linux":
                return "Linux";
            case "windows":
                return "Win";
            case "macos":
                return "Mac";
            case "ios":
            case "android":
            default:
                return "Unsupported";
        }
    }

    setContext('toast', { TriggerError, TriggerWarning, TriggerSuccess, backoffDownload, wait, GetOS });

    onMount(() => {										
        autoModeWatcher();
    })
</script>

<Toast />
<slot></slot>